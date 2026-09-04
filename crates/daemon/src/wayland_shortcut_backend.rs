use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use uuid::Uuid;
use zbus::{
    Connection, MatchRule, MessageStream, Proxy,
    message::Type as MessageType,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Str, Value},
};

use crate::shortcut_backend::{Shortcut, ShortcutBackend, ShortcutError, ShortcutKey};

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";

const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

const GLOBAL_SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";

const HOST_REGISTRY_INTERFACE: &str = "org.freedesktop.host.portal.Registry";

const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";

const SESSION_INTERFACE: &str = "org.freedesktop.portal.Session";

const REQUEST_PATH_PREFIX: &str = "/org/freedesktop/portal/desktop/request";

pub const POOKIE_APPLICATION_ID: &str = "io.github.riyanj220.PookiePaste";

pub const POOKIE_DESKTOP_FILE_ID: &str = "io.github.riyanj220.PookiePaste.desktop";

const CLIPBOARD_HISTORY_SHORTCUT_ID: &str = "clipboard-history";

const CLIPBOARD_HISTORY_DESCRIPTION: &str = "Open clipboard history";

const CREATE_SESSION_TIMEOUT: Duration = Duration::from_secs(15);

/*
 * BindShortcuts may legitimately involve desktop UI and user interaction,
 * so give it substantially more time than CreateSession.
 */
const BIND_SHORTCUTS_TIMEOUT: Duration = Duration::from_secs(120);

type ActivationResult = Result<(), ShortcutError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaylandShortcutCapability {
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundShortcut {
    pub id: String,
    pub trigger_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandShortcutActivation {
    pub shortcut_id: String,
    pub timestamp: u64,
    pub activation_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandShortcutDeactivation {
    pub shortcut_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostApplicationRegistration {
    Registered,
    Unavailable,
}

#[derive(Clone)]
pub struct WaylandShortcutSession {
    connection: Connection,
    session_handle: OwnedObjectPath,
}

/*
 * Represents one asynchronous XDG Desktop Portal Request.
 *
 * The Response subscription is installed during prepare(), before the
 * portal method itself is invoked. This is critical: a fast portal may
 * emit Request::Response immediately, and subscribing only after the
 * method returns introduces a race.
 */
struct PortalRequest {
    handle_token: String,
    expected_path: OwnedObjectPath,
    response_stream: MessageStream,
}

pub struct WaylandShortcutBackend {
    runtime: tokio::runtime::Runtime,

    session: Option<WaylandShortcutSession>,

    activation_receiver: Option<Receiver<ActivationResult>>,

    registered: bool,
}

impl PortalRequest {
    async fn prepare(connection: &Connection, token_prefix: &str) -> Result<Self, ShortcutError> {
        let handle_token = next_token(token_prefix);

        let expected_path = expected_request_path(connection, &handle_token)?;

        /*
         * Install the D-Bus match rule before invoking the portal method.
         *
         * This is what removes the Request::Response subscription race
         * for standards-compliant portals using predictable request paths.
         */
        let response_stream = response_stream_for_path(connection, &expected_path.as_ref()).await?;

        Ok(Self {
            handle_token,
            expected_path,
            response_stream,
        })
    }

    fn handle_token(&self) -> &str {
        &self.handle_token
    }

    async fn finish(
        self,
        connection: &Connection,
        returned_handle: OwnedObjectPath,
        operation: &str,
        timeout_duration: Duration,
    ) -> Result<(u32, HashMap<String, OwnedValue>), ShortcutError> {
        /*
         * Modern/spec-compliant portals should return exactly the request
         * path predicted from the D-Bus sender name and handle_token.
         *
         * Older implementations may return a different request path.
         * In that case, establish a bounded compatibility subscription
         * against the actual returned path.
         *
         * We cannot retroactively eliminate the race for a legacy portal
         * that uses an unpredictable path and emits Response before
         * returning that path.
         */
        let actual_path = returned_handle.clone();

        let mut stream = if returned_handle == self.expected_path {
            self.response_stream
        } else {
            response_stream_for_path(connection, &returned_handle.as_ref()).await?
        };

        let next = tokio::time::timeout(timeout_duration, stream.next()).await;

        let message = match next {
            Ok(Some(Ok(message))) => message,

            Ok(Some(Err(error))) => {
                return Err(ShortcutError::Failed(format!(
                    "{operation} response stream failed: {error}"
                )));
            }

            Ok(None) => {
                return Err(ShortcutError::Failed(format!(
                    "{operation} response stream ended unexpectedly"
                )));
            }

            Err(_) => {
                /*
                 * Timeout cleanup is best-effort. A cleanup failure must
                 * never replace the original timeout error.
                 */
                close_portal_request(connection, &actual_path.as_ref()).await;

                return Err(ShortcutError::TimedOut(format!(
                    "{operation} did not complete within {} seconds",
                    timeout_duration.as_secs(),
                )));
            }
        };

        let (response, results): (u32, HashMap<String, OwnedValue>) =
            message.body().deserialize().map_err(|error| {
                ShortcutError::Failed(format!(
                    "failed to decode {operation} portal response: {error}"
                ))
            })?;

        Ok((response, results))
    }
}

impl WaylandShortcutSession {
    pub fn session_handle(&self) -> ObjectPath<'_> {
        self.session_handle.as_ref()
    }

    pub async fn bind_clipboard_history(
        &self,
        preferred_trigger: &str,
    ) -> Result<Vec<BoundShortcut>, ShortcutError> {
        bind_shortcuts(
            &self.connection,
            &self.session_handle.as_ref(),
            preferred_trigger,
        )
        .await
    }

    pub async fn wait_for_clipboard_history_activation(
        &self,
    ) -> Result<WaylandShortcutActivation, ShortcutError> {
        wait_for_activation(&self.connection, &self.session_handle.as_ref()).await
    }

    pub async fn wait_for_clipboard_history_deactivation(
        &self,
    ) -> Result<WaylandShortcutDeactivation, ShortcutError> {
        wait_for_deactivation(&self.connection, &self.session_handle.as_ref()).await
    }

    pub async fn close(&self) -> Result<(), ShortcutError> {
        let proxy = Proxy::new(
            &self.connection,
            PORTAL_DESTINATION,
            self.session_handle.as_ref(),
            SESSION_INTERFACE,
        )
        .await
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to create portal session proxy: {error}"))
        })?;

        proxy
            .call::<_, _, ()>("Close", &())
            .await
            .map_err(|error| {
                ShortcutError::Failed(format!("failed to close GlobalShortcuts session: {error}"))
            })?;

        Ok(())
    }

    pub async fn run_activation_loop(
        &self,
        sender: Sender<ActivationResult>,
        ready: oneshot::Sender<Result<(), ShortcutError>>,
    ) -> Result<(), ShortcutError> {
        let proxy = match Proxy::new(
            &self.connection,
            PORTAL_DESTINATION,
            PORTAL_PATH,
            GLOBAL_SHORTCUTS_INTERFACE,
        )
        .await
        {
            Ok(proxy) => proxy,

            Err(_) => {
                let _ = ready.send(Err(ShortcutError::Unavailable));

                return Err(ShortcutError::Unavailable);
            }
        };

        let mut stream = match proxy.receive_signal("Activated").await {
            Ok(stream) => stream,

            Err(error) => {
                let message = format!("failed to subscribe to GlobalShortcuts Activated: {error}");

                let _ = ready.send(Err(ShortcutError::Failed(message.clone())));

                return Err(ShortcutError::Failed(message));
            }
        };

        /*
         * From this point onward the signal subscription exists.
         *
         * register() must not report success before this readiness
         * notification has been received.
         */
        let _ = ready.send(Ok(()));

        while let Some(message) = stream.next().await {
            let decoded: Result<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>), _> =
                message.body().deserialize();

            let (session_handle, shortcut_id, _timestamp, _options) = match decoded {
                Ok(decoded) => decoded,

                Err(error) => {
                    return Err(ShortcutError::Failed(format!(
                        "failed to decode GlobalShortcuts Activated signal: {error}"
                    )));
                }
            };

            if session_handle != self.session_handle {
                continue;
            }

            if shortcut_id != CLIPBOARD_HISTORY_SHORTCUT_ID {
                continue;
            }

            if sender.send(Ok(())).is_err() {
                /*
                 * The synchronous consumer has disappeared, so there
                 * is nothing left for this task to service.
                 */
                return Ok(());
            }
        }

        Err(ShortcutError::Failed(
            "GlobalShortcuts activation stream ended".to_string(),
        ))
    }
}

impl WaylandShortcutBackend {
    pub fn new() -> Result<Self, ShortcutError> {
        let runtime = tokio::runtime::Runtime::new().map_err(|error| {
            ShortcutError::Failed(format!(
                "failed to create Wayland shortcut runtime: {error}"
            ))
        })?;

        Ok(Self {
            runtime,
            session: None,
            activation_receiver: None,
            registered: false,
        })
    }

    fn close_session_after_failed_registration(&self, session: &WaylandShortcutSession) {
        /*
         * Cleanup must never replace the actual registration error.
         */
        let _ = self.runtime.block_on(session.close());
    }
}

impl ShortcutBackend for WaylandShortcutBackend {
    fn register(&mut self, shortcut: Shortcut) -> Result<(), ShortcutError> {
        if self.registered {
            return Err(ShortcutError::Failed(
                "Wayland shortcut backend is already registered".to_string(),
            ));
        }

        let preferred_trigger = portal_trigger(shortcut)?;

        let session = self.runtime.block_on(create_global_shortcuts_session())?;

        let bound = match self
            .runtime
            .block_on(session.bind_clipboard_history(&preferred_trigger))
        {
            Ok(bound) => bound,

            Err(error) => {
                self.close_session_after_failed_registration(&session);

                return Err(error);
            }
        };

        let shortcut_bound = bound
            .iter()
            .any(|shortcut| shortcut.id == CLIPBOARD_HISTORY_SHORTCUT_ID);

        if !shortcut_bound {
            self.close_session_after_failed_registration(&session);

            return Err(ShortcutError::Unavailable);
        }

        let (activation_sender, activation_receiver) = channel();

        let (ready_sender, ready_receiver) = oneshot::channel();

        let listener_session = session.clone();

        let terminal_sender = activation_sender.clone();

        self.runtime.handle().spawn(async move {
            let result = listener_session
                .run_activation_loop(activation_sender, ready_sender)
                .await;

            if let Err(error) = result {
                /*
                 * If startup failed, ready_sender already carries the
                 * startup error. Sending here is still safe: register()
                 * has not installed the receiver until readiness
                 * succeeds.
                 *
                 * Once startup has succeeded, this propagates terminal
                 * portal/listener failures to wait_for_activation().
                 */
                let _ = terminal_sender.send(Err(error));
            }
        });

        let readiness = self.runtime.block_on(async {
            ready_receiver.await.map_err(|_| {
                ShortcutError::Failed(
                    "Wayland activation listener stopped during startup".to_string(),
                )
            })?
        });

        if let Err(error) = readiness {
            self.close_session_after_failed_registration(&session);

            return Err(error);
        }

        /*
         * Only publish initialized state after:
         *
         *   session created
         *   shortcut bound
         *   Activated subscription established
         */
        self.session = Some(session);

        self.activation_receiver = Some(activation_receiver);

        self.registered = true;

        Ok(())
    }

    fn wait_for_activation(&mut self) -> Result<(), ShortcutError> {
        let receiver = self.activation_receiver.as_ref().ok_or_else(|| {
            ShortcutError::Failed("Wayland shortcut backend has not been registered".to_string())
        })?;

        receiver
            .recv()
            .map_err(|_| ShortcutError::Failed("Wayland activation listener stopped".to_string()))?
    }
}

pub async fn probe_global_shortcuts() -> Result<WaylandShortcutCapability, ShortcutError> {
    let connection = Connection::session().await.map_err(|error| {
        ShortcutError::Failed(format!("failed to connect to session D-Bus: {error}"))
    })?;

    let proxy = Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        GLOBAL_SHORTCUTS_INTERFACE,
    )
    .await
    .map_err(|error| {
        ShortcutError::Failed(format!(
            "failed to create GlobalShortcuts portal proxy: {error}"
        ))
    })?;

    let version: u32 = proxy
        .get_property("version")
        .await
        .map_err(|_| ShortcutError::Unavailable)?;

    Ok(WaylandShortcutCapability { version })
}

pub async fn create_global_shortcuts_session() -> Result<WaylandShortcutSession, ShortcutError> {
    let connection = Connection::session().await.map_err(|error| {
        ShortcutError::Failed(format!("failed to connect to session D-Bus: {error}"))
    })?;

    /*
     * Host application identity must be registered on the SAME
     * D-Bus connection that will subsequently use the portal.
     *
     * Registry support is intentionally optional. Current portal
     * documentation warns that this interface may eventually be
     * deprecated, so GlobalShortcuts must continue to work on
     * systems where Registry is unavailable.
     */
    match register_host_application(&connection).await {
        Ok(HostApplicationRegistration::Registered) => {
            info!(
                app_id = POOKIE_APPLICATION_ID,
                "registered Pookie host application identity with portal"
            );
        }

        Ok(HostApplicationRegistration::Unavailable) => {
            debug!(
                app_id = POOKIE_APPLICATION_ID,
                "host portal Registry unavailable; continuing without explicit application identity"
            );
        }

        Err(error) => {
            /*
             * Do not make GlobalShortcuts depend on Registry.
             *
             * Automatic portal application detection may still work,
             * and future portal versions may remove this interface.
             */
            warn!(
                app_id = POOKIE_APPLICATION_ID,
                error = ?error,
                "failed to register Pookie host application identity; continuing without it"
            );
        }
    }

    create_session_with_connection(connection).await
}

async fn register_host_application(
    connection: &Connection,
) -> Result<HostApplicationRegistration, ShortcutError> {
    let registry = Proxy::new(
        connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        HOST_REGISTRY_INTERFACE,
    )
    .await
    .map_err(|error| {
        ShortcutError::Failed(format!(
            "failed to create host portal Registry proxy: {error}"
        ))
    })?;

    /*
     * Registry version 1 currently defines no registration options,
     * but the API reserves an a{sv} options dictionary.
     */
    let options: HashMap<&str, Value<'_>> = HashMap::new();

    let result = registry
        .call::<_, _, ()>("Register", &(POOKIE_APPLICATION_ID, options))
        .await;

    match result {
        Ok(()) => Ok(HostApplicationRegistration::Registered),

        Err(error) if is_portal_method_unavailable(&error) => {
            Ok(HostApplicationRegistration::Unavailable)
        }

        Err(error) => Err(ShortcutError::Failed(format!(
            "host portal Registry.Register failed for {POOKIE_APPLICATION_ID}: {error}"
        ))),
    }
}

async fn create_session_with_connection(
    connection: Connection,
) -> Result<WaylandShortcutSession, ShortcutError> {
    let portal = Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        GLOBAL_SHORTCUTS_INTERFACE,
    )
    .await
    .map_err(|_| ShortcutError::Unavailable)?;

    /*
     * Prepare the Request response subscription BEFORE CreateSession.
     */
    let request = PortalRequest::prepare(&connection, "pookie_request").await?;

    let session_token = next_token("pookie_session");

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();

    options.insert("handle_token", Value::from(request.handle_token()));

    options.insert("session_handle_token", Value::from(session_token.as_str()));

    let request_handle: OwnedObjectPath = match portal.call("CreateSession", &(options,)).await {
        Ok(handle) => handle,

        Err(error) => {
            /*
             * The predicted request object may already have been created.
             * Closing it is best-effort and must not replace the method
             * error.
             */
            close_portal_request(&connection, &request.expected_path.as_ref()).await;

            return Err(map_portal_method_error(
                "GlobalShortcuts CreateSession",
                error,
            ));
        }
    };

    let (response, mut results) = request
        .finish(
            &connection,
            request_handle,
            "GlobalShortcuts CreateSession",
            CREATE_SESSION_TIMEOUT,
        )
        .await?;

    check_portal_response("GlobalShortcuts CreateSession", response)?;

    let session_handle = results.remove("session_handle").ok_or_else(|| {
        ShortcutError::Failed(
            "portal CreateSession response did not contain session_handle".to_string(),
        )
    })?;

    let session_handle: String = session_handle.try_into().map_err(|error| {
        ShortcutError::Failed(format!("invalid portal session_handle result: {error}"))
    })?;

    let session_handle = OwnedObjectPath::try_from(session_handle).map_err(|error| {
        ShortcutError::Failed(format!(
            "portal returned invalid session object path: {error}"
        ))
    })?;

    Ok(WaylandShortcutSession {
        connection,
        session_handle,
    })
}

async fn bind_shortcuts(
    connection: &Connection,
    session_handle: &ObjectPath<'_>,
    preferred_trigger: &str,
) -> Result<Vec<BoundShortcut>, ShortcutError> {
    let portal = Proxy::new(
        connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        GLOBAL_SHORTCUTS_INTERFACE,
    )
    .await
    .map_err(|_| ShortcutError::Unavailable)?;

    /*
     * Prepare the Response subscription before BindShortcuts.
     */
    let request = PortalRequest::prepare(connection, "pookie_bind").await?;

    let shortcuts = clipboard_history_shortcuts(preferred_trigger);

    let parent_window = "";

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();

    options.insert("handle_token", Value::from(request.handle_token()));

    let request_handle: OwnedObjectPath = match portal
        .call(
            "BindShortcuts",
            &(session_handle, shortcuts, parent_window, options),
        )
        .await
    {
        Ok(handle) => handle,

        Err(error) => {
            close_portal_request(connection, &request.expected_path.as_ref()).await;

            return Err(map_portal_method_error(
                "GlobalShortcuts BindShortcuts",
                error,
            ));
        }
    };

    let (response, mut results) = request
        .finish(
            connection,
            request_handle,
            "GlobalShortcuts BindShortcuts",
            BIND_SHORTCUTS_TIMEOUT,
        )
        .await?;

    check_portal_response("GlobalShortcuts BindShortcuts", response)?;

    let shortcuts = results.remove("shortcuts").ok_or_else(|| {
        ShortcutError::Failed("BindShortcuts response did not contain shortcuts".to_string())
    })?;

    let shortcuts: Vec<(String, HashMap<String, OwnedValue>)> =
        shortcuts.try_into().map_err(|error| {
            ShortcutError::Failed(format!("invalid BindShortcuts shortcuts result: {error}"))
        })?;

    let bound = shortcuts
        .into_iter()
        .map(|(id, mut properties)| {
            let trigger_description = properties
                .remove("trigger_description")
                .and_then(|value| String::try_from(value).ok());

            BoundShortcut {
                id,
                trigger_description,
            }
        })
        .collect();

    Ok(bound)
}

async fn wait_for_activation(
    connection: &Connection,
    expected_session: &ObjectPath<'_>,
) -> Result<WaylandShortcutActivation, ShortcutError> {
    let proxy = Proxy::new(
        connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        GLOBAL_SHORTCUTS_INTERFACE,
    )
    .await
    .map_err(|_| ShortcutError::Unavailable)?;

    let mut stream = proxy.receive_signal("Activated").await.map_err(|error| {
        ShortcutError::Failed(format!(
            "failed to subscribe to GlobalShortcuts Activated: {error}"
        ))
    })?;

    loop {
        let message = stream.next().await.ok_or_else(|| {
            ShortcutError::Failed("GlobalShortcuts Activated stream ended".to_string())
        })?;

        let (session_handle, shortcut_id, timestamp, mut options): (
            OwnedObjectPath,
            String,
            u64,
            HashMap<String, OwnedValue>,
        ) = message.body().deserialize().map_err(|error| {
            ShortcutError::Failed(format!(
                "failed to decode GlobalShortcuts Activated signal: {error}"
            ))
        })?;

        if session_handle.as_ref() != *expected_session {
            continue;
        }

        if shortcut_id != CLIPBOARD_HISTORY_SHORTCUT_ID {
            continue;
        }

        let activation_token = options
            .remove("activation_token")
            .and_then(|value| String::try_from(value).ok());

        return Ok(WaylandShortcutActivation {
            shortcut_id,
            timestamp,
            activation_token,
        });
    }
}

async fn wait_for_deactivation(
    connection: &Connection,
    expected_session: &ObjectPath<'_>,
) -> Result<WaylandShortcutDeactivation, ShortcutError> {
    let proxy = Proxy::new(
        connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        GLOBAL_SHORTCUTS_INTERFACE,
    )
    .await
    .map_err(|_| ShortcutError::Unavailable)?;

    let mut stream = proxy.receive_signal("Deactivated").await.map_err(|error| {
        ShortcutError::Failed(format!(
            "failed to subscribe to GlobalShortcuts Deactivated: {error}"
        ))
    })?;

    loop {
        let message = stream.next().await.ok_or_else(|| {
            ShortcutError::Failed("GlobalShortcuts Deactivated stream ended".to_string())
        })?;

        let (session_handle, shortcut_id, timestamp, _options): (
            OwnedObjectPath,
            String,
            u64,
            HashMap<String, OwnedValue>,
        ) = message.body().deserialize().map_err(|error| {
            ShortcutError::Failed(format!(
                "failed to decode GlobalShortcuts Deactivated signal: {error}"
            ))
        })?;

        if session_handle.as_ref() != *expected_session {
            continue;
        }

        if shortcut_id != CLIPBOARD_HISTORY_SHORTCUT_ID {
            continue;
        }

        return Ok(WaylandShortcutDeactivation {
            shortcut_id,
            timestamp,
        });
    }
}

fn clipboard_history_shortcuts(
    preferred_trigger: &str,
) -> Vec<(String, HashMap<String, OwnedValue>)> {
    let mut properties = HashMap::new();

    properties.insert(
        "description".to_string(),
        OwnedValue::from(Str::from(CLIPBOARD_HISTORY_DESCRIPTION)),
    );

    properties.insert(
        "preferred_trigger".to_string(),
        OwnedValue::from(Str::from(preferred_trigger)),
    );

    vec![(CLIPBOARD_HISTORY_SHORTCUT_ID.to_string(), properties)]
}

fn portal_trigger(shortcut: Shortcut) -> Result<String, ShortcutError> {
    let mut trigger = String::new();

    if shortcut.modifiers.control {
        trigger.push_str("<Ctrl>");
    }

    if shortcut.modifiers.alt {
        trigger.push_str("<Alt>");
    }

    if shortcut.modifiers.shift {
        trigger.push_str("<Shift>");
    }

    if shortcut.modifiers.super_key {
        trigger.push_str("<Super>");
    }

    match shortcut.key {
        ShortcutKey::Character(character) if character.is_ascii() => {
            trigger.push(character.to_ascii_lowercase());
        }

        ShortcutKey::Character(_) => {
            return Err(ShortcutError::Unavailable);
        }
    }

    Ok(trigger)
}

/*
 * Convert a D-Bus unique name into the sender component used by the
 * XDG Desktop Portal Request object-path convention.
 *
 * Example:
 *
 *     :1.823 -> 1_823
 */
fn request_sender_component(unique_name: &str) -> Result<String, ShortcutError> {
    let sender = unique_name.strip_prefix(':').ok_or_else(|| {
        ShortcutError::Failed(format!("invalid D-Bus unique name: {unique_name}"))
    })?;

    Ok(sender.replace('.', "_"))
}

/*
 * XDG Desktop Portal request paths are predictable from:
 *
 *     connection unique name + handle_token
 *
 * This lets us subscribe to Request::Response before invoking the
 * portal method and therefore removes the fast-response race.
 */
fn expected_request_path(
    connection: &Connection,
    handle_token: &str,
) -> Result<OwnedObjectPath, ShortcutError> {
    let unique_name = connection.unique_name().ok_or_else(|| {
        ShortcutError::Failed("session D-Bus connection has no unique name".to_string())
    })?;

    let sender = request_sender_component(&unique_name.as_ref())?;

    let path = format!("{REQUEST_PATH_PREFIX}/{sender}/{handle_token}");

    OwnedObjectPath::try_from(path).map_err(|error| {
        ShortcutError::Failed(format!("failed to construct portal request path: {error}"))
    })
}

/*
 * Register the Request::Response signal match directly on the D-Bus
 * connection.
 *
 * This function returns only after the match rule has been installed.
 */
async fn response_stream_for_path(
    connection: &Connection,
    request_path: &ObjectPath<'_>,
) -> Result<MessageStream, ShortcutError> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .path(request_path.to_owned())
        .map_err(|error| {
            ShortcutError::Failed(format!(
                "failed to build portal request path match: {error}"
            ))
        })?
        .interface(REQUEST_INTERFACE)
        .map_err(|error| {
            ShortcutError::Failed(format!(
                "failed to build portal request interface match: {error}"
            ))
        })?
        .member("Response")
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to build portal Response match: {error}"))
        })?
        .build();

    MessageStream::for_match_rule(rule, connection, Some(1))
        .await
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to subscribe to portal Response: {error}"))
        })
}

/*
 * Abort an outstanding portal request.
 *
 * Cleanup is deliberately best-effort: failure to close the request
 * must never replace the original timeout/method error.
 */
async fn close_portal_request(connection: &Connection, request_path: &ObjectPath<'_>) {
    let Ok(proxy) = Proxy::new(
        connection,
        PORTAL_DESTINATION,
        request_path,
        REQUEST_INTERFACE,
    )
    .await
    else {
        return;
    };

    let _ = proxy.call::<_, _, ()>("Close", &()).await;
}

fn check_portal_response(operation: &str, response: u32) -> Result<(), ShortcutError> {
    match response {
        0 => Ok(()),

        1 => Err(ShortcutError::Cancelled),

        2 => Err(ShortcutError::Failed(format!(
            "{operation} failed in the portal"
        ))),

        other => Err(ShortcutError::Failed(format!(
            "{operation} returned unknown response code {other}"
        ))),
    }
}

fn is_portal_method_unavailable(error: &zbus::Error) -> bool {
    let message = error.to_string();

    message.contains("UnknownMethod")
        || message.contains("UnknownInterface")
        || message.contains("No such interface")
}

fn map_portal_method_error(operation: &str, error: zbus::Error) -> ShortcutError {
    if is_portal_method_unavailable(&error) {
        ShortcutError::Unavailable
    } else {
        ShortcutError::Failed(format!("{operation} failed: {error}"))
    }
}

fn next_token(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut_backend::ShortcutModifiers;

    #[test]
    fn uses_stable_application_identity() {
        assert_eq!(POOKIE_APPLICATION_ID, "io.github.riyanj220.PookiePaste",);

        assert_eq!(
            POOKIE_DESKTOP_FILE_ID,
            "io.github.riyanj220.PookiePaste.desktop",
        );

        assert_eq!(
            POOKIE_DESKTOP_FILE_ID
                .strip_suffix(".desktop")
                .expect("desktop file id should end in .desktop"),
            POOKIE_APPLICATION_ID,
        );
    }

    #[test]
    fn converts_super_v_to_portal_trigger() {
        let trigger = portal_trigger(Shortcut::super_v()).unwrap();

        assert_eq!(trigger, "<Super>v");
    }

    #[test]
    fn converts_ctrl_alt_v_to_portal_trigger() {
        let trigger = portal_trigger(Shortcut::new(
            ShortcutKey::Character('v'),
            ShortcutModifiers {
                control: true,
                alt: true,
                ..ShortcutModifiers::NONE
            },
        ))
        .unwrap();

        assert_eq!(trigger, "<Ctrl><Alt>v");
    }

    #[test]
    fn converts_dbus_unique_name_to_request_sender() {
        assert_eq!(request_sender_component(":1.823").unwrap(), "1_823",);
    }

    #[test]
    fn rejects_non_unique_dbus_name_for_request_sender() {
        assert!(matches!(
            request_sender_component("1.823"),
            Err(ShortcutError::Failed(_)),
        ));
    }

    #[test]
    fn portal_tokens_are_object_path_safe() {
        let token = next_token("pookie_request");

        assert!(token.starts_with("pookie_request_"));

        assert!(!token.contains('-'));

        assert!(
            token
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        );
    }

    #[test]
    fn portal_tokens_are_unique() {
        assert_ne!(next_token("pookie_request"), next_token("pookie_request"),);
    }

    #[test]
    fn successful_portal_response_is_accepted() {
        assert!(check_portal_response("test", 0).is_ok());
    }

    #[test]
    fn cancelled_portal_response_is_cancelled() {
        assert!(matches!(
            check_portal_response("test", 1),
            Err(ShortcutError::Cancelled),
        ));
    }

    #[test]
    fn failed_portal_response_is_failure() {
        assert!(matches!(
            check_portal_response("test", 2),
            Err(ShortcutError::Failed(_)),
        ));
    }

    #[test]
    fn unknown_portal_response_is_failure() {
        assert!(matches!(
            check_portal_response("test", 99),
            Err(ShortcutError::Failed(_)),
        ));
    }
}
