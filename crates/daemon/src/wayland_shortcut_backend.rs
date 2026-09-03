use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use futures_util::StreamExt;
use zbus::{
    Connection, Proxy,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Str, Value},
};

use crate::shortcut_backend::{Shortcut, ShortcutBackend, ShortcutError, ShortcutKey};

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";

const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

const GLOBAL_SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";

const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";

const CLIPBOARD_HISTORY_SHORTCUT_ID: &str = "clipboard-history";

const CLIPBOARD_HISTORY_DESCRIPTION: &str = "Open clipboard history";

static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);

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

#[derive(Clone)]
pub struct WaylandShortcutSession {
    connection: Connection,
    session_handle: OwnedObjectPath,
}

pub struct WaylandShortcutBackend {
    runtime: tokio::runtime::Runtime,

    session: Option<WaylandShortcutSession>,

    activation_receiver: Option<Receiver<()>>,

    registered: bool,
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

    pub async fn run_activation_loop(&self, sender: Sender<()>) -> Result<(), ShortcutError> {
        let proxy = Proxy::new(
            &self.connection,
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

        while let Some(message) = stream.next().await {
            let (session_handle, shortcut_id, _timestamp, _options): (
                OwnedObjectPath,
                String,
                u64,
                HashMap<String, OwnedValue>,
            ) = message.body().deserialize().map_err(|error| {
                ShortcutError::Failed(format!(
                    "failed to decode GlobalShortcuts Activated signal: {error}"
                ))
            })?;

            if session_handle != self.session_handle {
                continue;
            }

            if shortcut_id != CLIPBOARD_HISTORY_SHORTCUT_ID {
                continue;
            }

            if sender.send(()).is_err() {
                return Ok(());
            }
        }

        Err(ShortcutError::Failed(
            "GlobalShortcuts Activated stream ended".to_string(),
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

        let bound = self
            .runtime
            .block_on(session.bind_clipboard_history(&preferred_trigger))?;

        let shortcut_bound = bound
            .iter()
            .any(|shortcut| shortcut.id == CLIPBOARD_HISTORY_SHORTCUT_ID);

        if !shortcut_bound {
            return Err(ShortcutError::Unavailable);
        }

        let (sender, receiver) = channel();

        let listener_session = session.clone();

        let handle = self.runtime.handle().clone();

        handle.spawn(async move {
            let _ = listener_session.run_activation_loop(sender).await;
        });

        self.session = Some(session);

        self.activation_receiver = Some(receiver);

        self.registered = true;

        Ok(())
    }

    fn wait_for_activation(&mut self) -> Result<(), ShortcutError> {
        let receiver = self.activation_receiver.as_ref().ok_or_else(|| {
            ShortcutError::Failed("Wayland shortcut backend has not been registered".to_string())
        })?;

        receiver.recv().map_err(|_| {
            ShortcutError::Failed("Wayland shortcut activation channel closed".to_string())
        })
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

    create_session_with_connection(connection).await
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

    let handle_token = next_token("pookie_request");

    let session_token = next_token("pookie_session");

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();

    options.insert("handle_token", Value::from(handle_token.as_str()));

    options.insert("session_handle_token", Value::from(session_token.as_str()));

    let request_handle: OwnedObjectPath = portal
        .call("CreateSession", &(options,))
        .await
        .map_err(|error| map_portal_method_error("GlobalShortcuts CreateSession", error))?;

    let (response, mut results) =
        wait_for_request_response(&connection, &request_handle.as_ref()).await?;

    if response != 0 {
        return Err(ShortcutError::Unavailable);
    }

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

    let handle_token = next_token("pookie_bind");

    let shortcuts = clipboard_history_shortcuts(preferred_trigger);

    let parent_window = "";

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();

    options.insert("handle_token", Value::from(handle_token.as_str()));

    let request_handle: OwnedObjectPath = portal
        .call(
            "BindShortcuts",
            &(session_handle, shortcuts, parent_window, options),
        )
        .await
        .map_err(|error| map_portal_method_error("GlobalShortcuts BindShortcuts", error))?;

    let (response, mut results) =
        wait_for_request_response(connection, &request_handle.as_ref()).await?;

    if response != 0 {
        return Err(ShortcutError::Unavailable);
    }

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

async fn wait_for_request_response(
    connection: &Connection,
    request_handle: &ObjectPath<'_>,
) -> Result<(u32, HashMap<String, OwnedValue>), ShortcutError> {
    let proxy = Proxy::new(
        connection,
        PORTAL_DESTINATION,
        request_handle,
        REQUEST_INTERFACE,
    )
    .await
    .map_err(|error| {
        ShortcutError::Failed(format!("failed to create portal request proxy: {error}"))
    })?;

    let mut stream = proxy.receive_signal("Response").await.map_err(|error| {
        ShortcutError::Failed(format!("failed to subscribe to portal response: {error}"))
    })?;

    let message = stream.next().await.ok_or_else(|| {
        ShortcutError::Failed("portal request ended without a response".to_string())
    })?;

    let (response, results): (u32, HashMap<String, OwnedValue>) =
        message.body().deserialize().map_err(|error| {
            ShortcutError::Failed(format!("failed to decode portal response: {error}"))
        })?;

    Ok((response, results))
}

fn map_portal_method_error(operation: &str, error: zbus::Error) -> ShortcutError {
    let message = error.to_string();

    if message.contains("UnknownMethod")
        || message.contains("UnknownInterface")
        || message.contains("No such interface")
    {
        ShortcutError::Unavailable
    } else {
        ShortcutError::Failed(format!("{operation} failed: {error}"))
    }
}

fn next_token(prefix: &str) -> String {
    let counter = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{prefix}_{}_{}", std::process::id(), counter,)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut_backend::ShortcutModifiers;

    #[test]
    fn converts_super_v_to_portal_trigger() {
        let trigger = portal_trigger(Shortcut::super_v()).unwrap();

        assert_eq!(trigger, "<Super>v",);
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

        assert_eq!(trigger, "<Ctrl><Alt>v",);
    }
}
