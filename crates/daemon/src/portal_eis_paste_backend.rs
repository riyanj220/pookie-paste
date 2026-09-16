use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::{
        fd::{FromRawFd, IntoRawFd},
        unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            net::UnixStream,
        },
    },
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::Duration,
};

use ashpd::desktop::{
    PersistMode, Session,
    remote_desktop::{ConnectToEISOptions, DeviceType, RemoteDesktop, SelectDevicesOptions},
};

use futures_util::StreamExt;

use reis::{
    ei,
    event::{Device, DeviceCapability, EiEvent},
    tokio::EiConvertEventStream,
};

use tokio::sync::mpsc as tokio_mpsc;

use crate::paste_backend::{PasteBackend, PasteCapability, PasteError};

const KEY_LEFTCTRL: u32 = 29;
const KEY_V: u32 = 47;

const EIS_CLIENT_NAME: &str = "Pookie Paste";

const DEVICE_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);

const PASTE_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

const KEY_INTERVAL: Duration = Duration::from_millis(30);

const RESTORE_TOKEN_FILE: &str = "remote-desktop.restore-token";
const RESTORE_TOKEN_TEMP_FILE: &str = ".remote-desktop.restore-token.tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum PortalEisHealth {
    Ready = 1,
    Paused = 2,
    Recovering = 3,
    Failed = 4,
}

impl PortalEisHealth {
    fn from_atomic(value: u8) -> Self {
        match value {
            1 => Self::Ready,
            2 => Self::Paused,
            3 => Self::Recovering,
            _ => Self::Failed,
        }
    }
}

fn health_capability(health: PortalEisHealth) -> PasteCapability {
    match health {
        PortalEisHealth::Ready => PasteCapability::Direct,
        PortalEisHealth::Paused | PortalEisHealth::Recovering | PortalEisHealth::Failed => {
            PasteCapability::ClipboardOnly
        }
    }
}

const RECONNECT_DELAYS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];

pub struct PortalEisPasteBackend {
    sender: tokio_mpsc::UnboundedSender<WorkerCommand>,
    health: Arc<AtomicU8>,
}

enum WorkerCommand {
    Paste {
        reply: SyncSender<Result<(), PasteError>>,
    },

    Shutdown {
        reply: SyncSender<()>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionRunOutcome {
    Reconnect,
    BackendDropped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventOutcome {
    Continue,
    Reconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OfflineCommandOutcome {
    Continue,
    Shutdown,
}

struct PortalEisSession {
    /*
     * Keep the portal/session alive for the complete
     * lifetime of the EI connection.
     */
    _portal: RemoteDesktop,

    _session: Session<RemoteDesktop>,

    /*
     * reis updates this connection's server serial as
     * incoming events are processed.
     */
    connection: reis::event::Connection,

    events: EiConvertEventStream,

    keyboard_device: Device,

    keyboard: ei::Keyboard,

    resumed: bool,

    /*
     * Emulation is intentionally long-lived for the active
     * RemoteDesktop/EIS session. We start it when the keyboard
     * device resumes and keep it active across individual paste
     * requests instead of starting/stopping around every Ctrl+V.
     */
    emulating: bool,

    next_sequence: u32,

    health: Arc<AtomicU8>,
}

impl PortalEisPasteBackend {
    pub fn new() -> Result<Self, PasteError> {
        let (command_sender, command_receiver) = tokio_mpsc::unbounded_channel();

        let (init_sender, init_receiver) = mpsc::sync_channel(1);

        let health = Arc::new(AtomicU8::new(PortalEisHealth::Paused as u8));

        let worker_health = Arc::clone(&health);

        thread::Builder::new()
            .name("pookie-portal-eis".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,

                    Err(error) => {
                        worker_health.store(PortalEisHealth::Failed as u8, Ordering::Release);

                        let _ = init_sender.send(Err(PasteError::Failed(format!(
                            "failed to create Portal/EIS runtime: {error}"
                        ))));

                        return;
                    }
                };

                runtime.block_on(async move {
                    match PortalEisSession::connect(Arc::clone(&worker_health)).await {
                        Ok(session) => {
                            worker_health.store(PortalEisHealth::Ready as u8, Ordering::Release);

                            let _ = init_sender.send(Ok(()));

                            run_worker(session, command_receiver, worker_health).await;
                        }

                        Err(error) => {
                            worker_health.store(PortalEisHealth::Failed as u8, Ordering::Release);

                            let _ = init_sender.send(Err(error));
                        }
                    }
                });
            })
            .map_err(|error| {
                PasteError::Failed(format!("failed to start Portal/EIS worker: {error}"))
            })?;

        /*
         * First-time initialization may show the desktop's
         * RemoteDesktop permission dialog.
         *
         * We intentionally do not apply a tiny initialization
         * timeout here because the user must be allowed to
         * approve or reject that portal request.
         */
        match init_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                sender: command_sender,
                health,
            }),

            Ok(Err(error)) => Err(error),

            Err(error) => Err(PasteError::Failed(format!(
                "Portal/EIS worker disconnected during initialization: {error}"
            ))),
        }
    }

    fn request_paste(&self) -> Result<(), PasteError> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);

        self.sender
            .send(WorkerCommand::Paste {
                reply: reply_sender,
            })
            .map_err(|_| {
                self.health
                    .store(PortalEisHealth::Failed as u8, Ordering::Release);

                PasteError::Unavailable
            })?;

        match reply_receiver.recv_timeout(PASTE_COMMAND_TIMEOUT) {
            Ok(result) => result,

            Err(error) => {
                self.health
                    .store(PortalEisHealth::Failed as u8, Ordering::Release);

                tracing::warn!(
                    error = ?error,
                    "Portal/EIS paste worker stopped responding"
                );

                Err(PasteError::Unavailable)
            }
        }
    }

    fn request_shutdown(&self) {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);

        if self
            .sender
            .send(WorkerCommand::Shutdown {
                reply: reply_sender,
            })
            .is_err()
        {
            /*
             * Worker already stopped.
             */
            self.health
                .store(PortalEisHealth::Failed as u8, Ordering::Release);

            return;
        }

        if let Err(error) = reply_receiver.recv_timeout(WORKER_SHUTDOWN_TIMEOUT) {
            tracing::debug!(
                error = ?error,
                "Portal/EIS worker did not acknowledge shutdown"
            );
        }

        self.health
            .store(PortalEisHealth::Failed as u8, Ordering::Release);
    }
}

impl PasteBackend for PortalEisPasteBackend {
    fn capability(&self) -> PasteCapability {
        let health = PortalEisHealth::from_atomic(self.health.load(Ordering::Acquire));

        health_capability(health)
    }

    fn paste(&self) -> Result<(), PasteError> {
        if self.capability() != PasteCapability::Direct {
            return Err(PasteError::Unavailable);
        }

        self.request_paste()
    }

    fn shutdown(&self) {
        self.request_shutdown();
    }
}

async fn run_worker(
    mut session: PortalEisSession,
    mut receiver: tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    health: Arc<AtomicU8>,
) {
    loop {
        match session.run(&mut receiver).await {
            SessionRunOutcome::BackendDropped => {
                health.store(PortalEisHealth::Failed as u8, Ordering::Release);

                break;
            }

            SessionRunOutcome::Reconnect => {
                health.store(PortalEisHealth::Recovering as u8, Ordering::Release);

                tracing::warn!("Portal/EIS session lost; attempting recovery");
            }
        }

        drop(session);

        let mut retry_index = 0usize;

        loop {
            let delay = RECONNECT_DELAYS[retry_index.min(RECONNECT_DELAYS.len() - 1)];

            tracing::info!(
                retry_seconds = delay.as_secs(),
                "waiting before Portal/EIS reconnect"
            );

            if !wait_for_reconnect_delay(&mut receiver, delay).await {
                health.store(PortalEisHealth::Failed as u8, Ordering::Release);

                return;
            }

            tracing::info!("attempting Portal/EIS reconnection");

            let reconnect_result = reconnect_session(&mut receiver, Arc::clone(&health)).await;

            let Some(reconnect_result) = reconnect_result else {
                health.store(PortalEisHealth::Failed as u8, Ordering::Release);

                return;
            };

            match reconnect_result {
                Ok(new_session) => {
                    /*
                     * Never execute paste commands that were queued while the
                     * connection was unavailable.
                     */
                    if !reject_pending_commands(&mut receiver) {
                        health.store(PortalEisHealth::Failed as u8, Ordering::Release);

                        return;
                    }

                    health.store(PortalEisHealth::Ready as u8, Ordering::Release);

                    tracing::info!("Portal/EIS direct paste recovered");

                    session = new_session;

                    break;
                }

                Err(error) => {
                    health.store(PortalEisHealth::Recovering as u8, Ordering::Release);

                    tracing::warn!(
                        error = ?error,
                        "Portal/EIS reconnection failed"
                    );

                    retry_index = retry_index.saturating_add(1);
                }
            }
        }
    }
}

fn handle_offline_command(command: WorkerCommand) -> OfflineCommandOutcome {
    match command {
        WorkerCommand::Paste { reply } => {
            let _ = reply.send(Err(PasteError::Unavailable));

            OfflineCommandOutcome::Continue
        }

        WorkerCommand::Shutdown { reply } => {
            let _ = reply.send(());

            OfflineCommandOutcome::Shutdown
        }
    }
}

fn reject_pending_commands(receiver: &mut tokio_mpsc::UnboundedReceiver<WorkerCommand>) -> bool {
    loop {
        match receiver.try_recv() {
            Ok(command) => match handle_offline_command(command) {
                OfflineCommandOutcome::Continue => {}

                OfflineCommandOutcome::Shutdown => {
                    return false;
                }
            },

            Err(tokio_mpsc::error::TryRecvError::Empty) => {
                return true;
            }

            Err(tokio_mpsc::error::TryRecvError::Disconnected) => {
                return false;
            }
        }
    }
}

async fn wait_for_reconnect_delay(
    receiver: &mut tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    delay: Duration,
) -> bool {
    let sleep = tokio::time::sleep(delay);

    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => {
                return true;
            }

            command = receiver.recv() => {
                match command {
                    Some(command) => {
                        match handle_offline_command(command) {
                            OfflineCommandOutcome::Continue => {}

                            OfflineCommandOutcome::Shutdown => {
                                return false;
                            }
                        }
                    }

                    None => {
                        return false;
                    }
                }
            }
        }
    }
}

async fn reconnect_session(
    receiver: &mut tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    health: Arc<AtomicU8>,
) -> Option<Result<PortalEisSession, PasteError>> {
    let connect_future = PortalEisSession::connect(health);

    tokio::pin!(connect_future);

    loop {
        tokio::select! {
            result = &mut connect_future => {
                return Some(result);
            }

            command = receiver.recv() => {
                match command {
                    Some(command) => {
                        match handle_offline_command(command) {
                            OfflineCommandOutcome::Continue => {}

                            OfflineCommandOutcome::Shutdown => {
                                return None;
                            }
                        }
                    }

                    None => {
                        return None;
                    }
                }
            }
        }
    }
}

impl PortalEisSession {
    async fn connect(health: Arc<AtomicU8>) -> Result<Self, PasteError> {
        tracing::info!("initializing Wayland Portal/EIS direct paste");

        let portal = RemoteDesktop::new().await.map_err(|error| {
            PasteError::Failed(format!(
                "failed to create RemoteDesktop portal proxy: {error}"
            ))
        })?;

        if portal.version() < 2 {
            return Err(PasteError::Unavailable);
        }

        let available_devices = portal.available_device_types().await.map_err(|error| {
            PasteError::Failed(format!(
                "failed to query RemoteDesktop device capabilities: {error}"
            ))
        })?;

        if !available_devices.contains(DeviceType::Keyboard) {
            return Err(PasteError::Unavailable);
        }

        let session = portal
            .create_session(Default::default())
            .await
            .map_err(|error| {
                PasteError::Failed(format!("failed to create RemoteDesktop session: {error}"))
            })?;

        let restore_token = load_restore_token();

        let mut select_options = SelectDevicesOptions::default()
            .set_devices(Some(DeviceType::Keyboard.into()))
            .set_persist_mode(PersistMode::ExplicitlyRevoked);

        if let Some(token) = restore_token.as_deref() {
            tracing::info!("restoring persisted RemoteDesktop authorization");

            select_options = select_options.set_restore_token(Some(token));
        } else {
            tracing::info!(
                "no persisted RemoteDesktop authorization; portal approval may be required"
            );
        }

        portal
            .select_devices(&session, select_options)
            .await
            .map_err(|error| {
                PasteError::Failed(format!(
                    "failed to issue RemoteDesktop SelectDevices request: {error}"
                ))
            })?
            .response()
            .map_err(|error| {
                PasteError::Failed(format!(
                    "RemoteDesktop SelectDevices was rejected or cancelled: {error}"
                ))
            })?;

        let start_response = portal
            .start(&session, None, Default::default())
            .await
            .map_err(|error| {
                PasteError::Failed(format!(
                    "failed to issue RemoteDesktop Start request: {error}"
                ))
            })?
            .response()
            .map_err(|error| {
                PasteError::Failed(format!(
                    "RemoteDesktop Start was rejected or cancelled: {error}"
                ))
            })?;

        if !start_response.devices().contains(DeviceType::Keyboard) {
            return Err(PasteError::Unavailable);
        }

        match start_response.restore_token() {
            Some(token) => {
                if let Err(error) = save_restore_token(token) {
                    tracing::warn!(
                        error = ?error,
                        "failed to persist RemoteDesktop restore token"
                    );
                } else {
                    tracing::info!("RemoteDesktop authorization token persisted");
                }
            }

            None => {
                if let Err(error) = clear_restore_token() {
                    tracing::warn!(
                        error = ?error,
                        "failed to clear stale RemoteDesktop restore token"
                    );
                }
            }
        }

        let eis_fd = portal
            .connect_to_eis(&session, ConnectToEISOptions::default())
            .await
            .map_err(|error| {
                PasteError::Failed(format!("RemoteDesktop ConnectToEIS failed: {error}"))
            })?;

        /*
         * Transfer ownership of the portal-provided FD
         * into the UnixStream exactly as proven by the
         * investigation probe.
         */
        let raw_fd = eis_fd.into_raw_fd();

        let socket = unsafe { UnixStream::from_raw_fd(raw_fd) };

        let context = ei::Context::new(socket)
            .map_err(|error| PasteError::Failed(format!("failed creating EI context: {error}")))?;

        let (connection, mut events) = context
            .handshake_tokio(EIS_CLIENT_NAME, ei::handshake::ContextType::Sender)
            .await
            .map_err(|error| PasteError::Failed(format!("EI handshake failed: {error}")))?;

        let seat = tokio::time::timeout(DEVICE_DISCOVERY_TIMEOUT, async {
            loop {
                let event = events
                    .next()
                    .await
                    .ok_or_else(|| {
                        PasteError::Failed(
                            "EI event stream ended before seat discovery".to_string(),
                        )
                    })?
                    .map_err(|error| {
                        PasteError::Failed(format!(
                            "failed receiving EI event during seat discovery: {error}"
                        ))
                    })?;

                match event {
                    EiEvent::SeatAdded(event) => {
                        return Ok::<_, PasteError>(event.seat);
                    }

                    EiEvent::Disconnected(event) => {
                        return Err(PasteError::Failed(format!(
                            "EIS disconnected during seat discovery: {event:?}"
                        )));
                    }

                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| PasteError::Failed("timed out waiting for EI seat".to_string()))??;

        seat.bind_capabilities(DeviceCapability::Keyboard.into());

        connection.flush().map_err(|error| {
            PasteError::Failed(format!(
                "failed flushing EI keyboard capability bind: {error}"
            ))
        })?;

        let keyboard_device = tokio::time::timeout(DEVICE_DISCOVERY_TIMEOUT, async {
            loop {
                let event = events
                    .next()
                    .await
                    .ok_or_else(|| {
                        PasteError::Failed(
                            "EI event stream ended before keyboard discovery".to_string(),
                        )
                    })?
                    .map_err(|error| {
                        PasteError::Failed(format!(
                            "failed receiving EI event during keyboard discovery: {error}"
                        ))
                    })?;

                match event {
                    EiEvent::DeviceAdded(event) => {
                        if event.device.has_capability(DeviceCapability::Keyboard) {
                            return Ok::<_, PasteError>(event.device);
                        }
                    }

                    EiEvent::Disconnected(event) => {
                        return Err(PasteError::Failed(format!(
                            "EIS disconnected during keyboard discovery: {event:?}"
                        )));
                    }

                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| {
            PasteError::Failed("timed out waiting for EI keyboard device".to_string())
        })??;

        let keyboard = keyboard_device.interface::<ei::Keyboard>().ok_or_else(|| {
            PasteError::Failed(
                "EI device reports keyboard capability without ei_keyboard interface".to_string(),
            )
        })?;

        let resume_serial = tokio::time::timeout(DEVICE_DISCOVERY_TIMEOUT, async {
            loop {
                let event = events
                    .next()
                    .await
                    .ok_or_else(|| {
                        PasteError::Failed(
                            "EI event stream ended before keyboard resume".to_string(),
                        )
                    })?
                    .map_err(|error| {
                        PasteError::Failed(format!(
                            "failed receiving EI event while waiting for keyboard resume: {error}"
                        ))
                    })?;

                match event {
                    EiEvent::DeviceResumed(event) if event.device == keyboard_device => {
                        return Ok::<_, PasteError>(event.serial);
                    }

                    EiEvent::DeviceRemoved(event) if event.device == keyboard_device => {
                        return Err(PasteError::Unavailable);
                    }

                    EiEvent::Disconnected(event) => {
                        return Err(PasteError::Failed(format!(
                            "EIS disconnected before keyboard resume: {event:?}"
                        )));
                    }

                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| {
            PasteError::Failed("timed out waiting for EI keyboard device to resume".to_string())
        })??;

        /*
         * Keep one emulation transaction active for the lifetime
         * of the resumed EIS keyboard device. Individual paste
         * requests only send key events.
         */
        let initial_sequence = 1;

        keyboard_device
            .device()
            .start_emulating(resume_serial, initial_sequence);

        connection.flush().map_err(|error| {
            PasteError::Failed(format!(
                "failed starting initial EI keyboard emulation: {error}"
            ))
        })?;

        tracing::info!(
            serial = resume_serial,
            sequence = initial_sequence,
            "Portal/EIS keyboard emulation started"
        );

        tracing::info!(
            device = ?keyboard_device.name(),
                       "Wayland Portal/EIS direct paste ready"
        );

        Ok(Self {
            _portal: portal,
            _session: session,
            connection,
            events,
            keyboard_device,
            keyboard,
            resumed: true,
            emulating: true,
            next_sequence: 2,
            health,
        })
    }

    async fn run(
        &mut self,
        receiver: &mut tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    ) -> SessionRunOutcome {
        loop {
            tokio::select! {
                command = receiver.recv() => {
                    match command {
                        Some(WorkerCommand::Paste { reply }) => {
                            let result = self.paste_ctrl_v().await;

                            let connection_failed =
                            matches!(result, Err(PasteError::Failed(_)));

                            let _ = reply.send(result);

                            if connection_failed {
                                self.health.store(
                                    PortalEisHealth::Recovering as u8,
                                    Ordering::Release,
                                );

                                self.stop_emulating_best_effort();

                                return SessionRunOutcome::Reconnect;
                            }
                        }

                        Some(WorkerCommand::Shutdown { reply }) => {
                            /*
                             * Explicitly end the active emulation transaction
                             * before dropping the portal/EIS objects.
                             */
                            self.stop_emulating_best_effort();

                            self.health
                            .store(PortalEisHealth::Failed as u8, Ordering::Release);

                            let _ = reply.send(());

                            tracing::info!("Portal/EIS paste worker shutting down");

                            return SessionRunOutcome::BackendDropped;
                        }

                        None => {
                            self.stop_emulating_best_effort();

                            return SessionRunOutcome::BackendDropped;
                        }
                    }
                }

                event = self.events.next() => {
                    match event {
                        Some(Ok(event)) => {
                            match self.handle_event(event) {
                                EventOutcome::Continue => {}

                                EventOutcome::Reconnect => {
                                    self.stop_emulating_best_effort();

                                    return SessionRunOutcome::Reconnect;
                                }
                            }
                        }

                        Some(Err(error)) => {
                            self.health.store(
                                PortalEisHealth::Recovering as u8,
                                Ordering::Release,
                            );

                            tracing::warn!(
                                error = ?error,
                                "Portal/EIS event stream failed"
                            );

                            self.stop_emulating_best_effort();

                            return SessionRunOutcome::Reconnect;
                        }

                        None => {
                            self.health.store(
                                PortalEisHealth::Recovering as u8,
                                Ordering::Release,
                            );

                            tracing::warn!("Portal/EIS event stream ended");

                            self.stop_emulating_best_effort();

                            return SessionRunOutcome::Reconnect;
                        }
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: EiEvent) -> EventOutcome {
        match event {
            EiEvent::DeviceResumed(event) if event.device == self.keyboard_device => {
                self.resumed = true;

                let sequence = self.take_next_sequence();

                self.keyboard_device
                    .device()
                    .start_emulating(event.serial, sequence);

                match self.connection.flush() {
                    Ok(()) => {
                        self.emulating = true;

                        self.health
                            .store(PortalEisHealth::Ready as u8, Ordering::Release);

                        tracing::info!(
                            serial = event.serial,
                            sequence,
                            "Portal/EIS keyboard emulation resumed"
                        );
                    }

                    Err(error) => {
                        self.emulating = false;

                        self.health
                            .store(PortalEisHealth::Recovering as u8, Ordering::Release);

                        tracing::error!(
                            error = ?error,
                            serial = event.serial,
                            sequence,
                            "failed restarting Portal/EIS keyboard emulation"
                        );

                        return EventOutcome::Reconnect;
                    }
                }
            }

            EiEvent::DevicePaused(event) if event.device == self.keyboard_device => {
                /*
                 * A paused logical device is reset to neutral state
                 * by EIS. Do not issue stop_emulating here; wait for
                 * DeviceResumed and begin a fresh emulation sequence.
                 */
                self.resumed = false;
                self.emulating = false;

                self.health
                    .store(PortalEisHealth::Paused as u8, Ordering::Release);

                tracing::debug!(serial = event.serial, "Portal/EIS keyboard paused");
            }

            EiEvent::DeviceRemoved(event) if event.device == self.keyboard_device => {
                self.resumed = false;
                self.emulating = false;

                self.health
                    .store(PortalEisHealth::Recovering as u8, Ordering::Release);

                tracing::warn!("Portal/EIS keyboard device removed");

                return EventOutcome::Reconnect;
            }

            EiEvent::Disconnected(event) => {
                self.resumed = false;
                self.emulating = false;

                self.health
                    .store(PortalEisHealth::Recovering as u8, Ordering::Release);

                tracing::warn!(
                    event = ?event,
                    "Portal/EIS disconnected"
                );

                return EventOutcome::Reconnect;
            }

            _ => {}
        }

        EventOutcome::Continue
    }

    fn take_next_sequence(&mut self) -> u32 {
        let sequence = self.next_sequence;

        self.next_sequence = self.next_sequence.wrapping_add(1);

        if self.next_sequence == 0 {
            self.next_sequence = 1;
        }

        sequence
    }

    async fn paste_ctrl_v(&mut self) -> Result<(), PasteError> {
        if !self.resumed || !self.emulating || !self.keyboard.is_alive() {
            return Err(PasteError::Unavailable);
        }

        let mut ctrl_down = false;
        let mut v_down = false;

        let result: Result<(), PasteError> = async {
            self.send_key_frame(KEY_LEFTCTRL, ei::keyboard::KeyState::Press)?;

            ctrl_down = true;

            tokio::time::sleep(KEY_INTERVAL).await;

            self.send_key_frame(KEY_V, ei::keyboard::KeyState::Press)?;

            v_down = true;

            tokio::time::sleep(KEY_INTERVAL).await;

            self.send_key_frame(KEY_V, ei::keyboard::KeyState::Released)?;

            v_down = false;

            tokio::time::sleep(KEY_INTERVAL).await;

            self.send_key_frame(KEY_LEFTCTRL, ei::keyboard::KeyState::Released)?;

            ctrl_down = false;

            Ok(())
        }
        .await;

        if let Err(error) = result {
            self.cleanup_pressed_keys(ctrl_down, v_down);

            return Err(error);
        }

        Ok(())
    }

    fn send_key_frame(&self, key: u32, state: ei::keyboard::KeyState) -> Result<(), PasteError> {
        self.keyboard.key(key, state);

        self.keyboard_device
            .device()
            .frame(self.connection.serial(), monotonic_time_micros());

        self.connection.flush().map_err(|error| {
            PasteError::Failed(format!("failed flushing EI keyboard frame: {error}"))
        })
    }

    fn cleanup_pressed_keys(&self, ctrl_down: bool, v_down: bool) {
        let device = self.keyboard_device.device();

        if v_down {
            self.keyboard.key(KEY_V, ei::keyboard::KeyState::Released);

            device.frame(self.connection.serial(), monotonic_time_micros());
        }

        if ctrl_down {
            self.keyboard
                .key(KEY_LEFTCTRL, ei::keyboard::KeyState::Released);

            device.frame(self.connection.serial(), monotonic_time_micros());
        }

        let _ = self.connection.flush();
    }

    fn stop_emulating_best_effort(&mut self) {
        if !self.resumed || !self.emulating || !self.keyboard.is_alive() {
            self.emulating = false;

            return;
        }

        self.keyboard_device
            .device()
            .stop_emulating(self.connection.serial());

        if let Err(error) = self.connection.flush() {
            tracing::debug!(
                error = ?error,
                "failed stopping Portal/EIS keyboard emulation during shutdown"
            );
        }

        self.emulating = false;
    }
}

fn monotonic_time_micros() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);

    (time.tv_sec as u64 * 1_000_000) + (time.tv_nsec as u64 / 1_000)
}

fn restore_token_path() -> Option<PathBuf> {
    if let Some(state_home) = env::var_os("XDG_STATE_HOME")
        && !state_home.is_empty()
    {
        return Some(
            PathBuf::from(state_home)
                .join("pookie-paste")
                .join(RESTORE_TOKEN_FILE),
        );
    }

    let home = env::var_os("HOME")?;

    if home.is_empty() {
        return None;
    }

    Some(
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("pookie-paste")
            .join(RESTORE_TOKEN_FILE),
    )
}

fn load_restore_token() -> Option<String> {
    let path = restore_token_path()?;

    match fs::read_to_string(&path) {
        Ok(value) => {
            let token = value.trim();

            if token.is_empty() {
                None
            } else {
                Some(token.to_string())
            }
        }

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,

        Err(error) => {
            tracing::warn!(
                error = ?error,
                path = %path.display(),
                           "failed to read RemoteDesktop restore token"
            );

            None
        }
    }
}

fn save_restore_token_at(path: &std::path::Path, token: &str) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent)?;

    let temp_path = parent.join(RESTORE_TOKEN_TEMP_FILE);

    /*
     * Never modify the live token in place.
     *
     * A restore token is single-use. If Pookie crashes while
     * replacing it, retaining the previous complete file is safer
     * than leaving a partially-written new token.
     */
    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temp_path)?;

        file.write_all(token.as_bytes())?;

        /*
         * sync_all() requests that file contents and metadata reach
         * stable storage before the atomic rename.
         */
        file.sync_all()?;

        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))?;

        fs::rename(&temp_path, path)?;

        /*
         * Persist the directory entry containing the rename as well.
         */
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }

        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    write_result
}

fn save_restore_token(token: &str) -> std::io::Result<()> {
    let Some(path) = restore_token_path() else {
        return Ok(());
    };

    save_restore_token_at(&path, token)
}

fn clear_restore_token() -> std::io::Result<()> {
    let Some(path) = restore_token_path() else {
        return Ok(());
    };

    match fs::remove_file(path) {
        Ok(()) => Ok(()),

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),

        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_health_is_direct() {
        assert_eq!(
            health_capability(PortalEisHealth::Ready),
            PasteCapability::Direct,
        );
    }

    #[test]
    fn paused_health_is_clipboard_only() {
        assert_eq!(
            health_capability(PortalEisHealth::Paused),
            PasteCapability::ClipboardOnly,
        );
    }

    #[test]
    fn recovering_health_is_clipboard_only() {
        assert_eq!(
            health_capability(PortalEisHealth::Recovering),
            PasteCapability::ClipboardOnly,
        );
    }

    #[test]
    fn reconnect_backoff_is_bounded() {
        assert_eq!(
            RECONNECT_DELAYS,
            [
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(5),
                Duration::from_secs(10),
            ],
        );
    }

    #[test]
    fn failed_health_is_clipboard_only() {
        assert_eq!(
            health_capability(PortalEisHealth::Failed),
            PasteCapability::ClipboardOnly,
        );
    }

    #[test]
    fn restore_token_file_name_is_stable() {
        assert_eq!(RESTORE_TOKEN_FILE, "remote-desktop.restore-token",);
    }

    #[test]
    fn linux_keycodes_are_stable() {
        assert_eq!(KEY_LEFTCTRL, 29);
        assert_eq!(KEY_V, 47);
    }
    #[test]
    fn atomic_token_write_stores_exact_content_and_private_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory =
            std::env::temp_dir().join(format!("pookie-paste-token-test-{}", uuid::Uuid::new_v4()));

        let path = directory.join(RESTORE_TOKEN_FILE);

        save_restore_token_at(&path, "token-one").expect("token write should succeed");

        let content = fs::read_to_string(&path).expect("token should be readable");
        assert_eq!(content, "token-one");

        let mode = fs::metadata(&path)
            .expect("token metadata should be readable")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(mode, 0o600);

        assert!(
            !directory.join(RESTORE_TOKEN_TEMP_FILE).exists(),
            "temporary token file should not remain after success",
        );

        fs::remove_dir_all(directory).expect("temporary test directory cleanup should succeed");
    }

    #[test]
    fn atomic_token_write_replaces_previous_token() {
        let directory =
            std::env::temp_dir().join(format!("pookie-paste-token-test-{}", uuid::Uuid::new_v4()));

        let path = directory.join(RESTORE_TOKEN_FILE);

        save_restore_token_at(&path, "token-one").expect("first token write should succeed");
        save_restore_token_at(&path, "token-two").expect("second token write should succeed");

        let content = fs::read_to_string(&path).expect("token should be readable");
        assert_eq!(content, "token-two");

        assert!(
            !directory.join(RESTORE_TOKEN_TEMP_FILE).exists(),
            "temporary token file should not remain after replacement",
        );

        fs::remove_dir_all(directory).expect("temporary test directory cleanup should succeed");
    }

    #[test]
    fn offline_shutdown_command_is_acknowledged() {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);

        let outcome = handle_offline_command(WorkerCommand::Shutdown {
            reply: reply_sender,
        });

        assert_eq!(outcome, OfflineCommandOutcome::Shutdown);
        assert!(
            reply_receiver
                .recv_timeout(Duration::from_millis(50))
                .is_ok()
        );
    }

    #[test]
    fn offline_paste_command_is_rejected_as_unavailable() {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);

        let outcome = handle_offline_command(WorkerCommand::Paste {
            reply: reply_sender,
        });

        assert_eq!(outcome, OfflineCommandOutcome::Continue);

        let result = reply_receiver
            .recv_timeout(Duration::from_millis(50))
            .expect("offline paste reply should arrive");

        assert!(matches!(result, Err(PasteError::Unavailable)));
    }
}
