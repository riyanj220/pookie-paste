use std::{
    sync::mpsc::{self, SyncSender},
    thread,
    time::Duration,
};

use tokio::sync::mpsc as tokio_mpsc;
use uuid::Uuid;
use zbus::{Connection, Proxy};

use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};

const POOKIE_SERVICE: &str = "io.github.riyanj220.PookiePaste";

const POOKIE_FOCUS_PATH: &str = "/io/github/riyanj220/PookiePaste/Focus";

const KGLOBALACCEL_SERVICE: &str = "org.kde.kglobalaccel";

const KGLOBALACCEL_PATH: &str = "/component/kwin";

const KGLOBALACCEL_INTERFACE: &str = "org.kde.kglobalaccel.Component";

const ACTION_CAPTURE: &str = "PookiePasteFocusCapture";

const ACTION_RESTORE: &str = "PookiePasteFocusRestore";

const ACTION_ACTIVE: &str = "PookiePasteFocusActive";

const WORKER_INIT_TIMEOUT: Duration = Duration::from_secs(2);

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(1);

pub struct KdeFocusBackend {
    sender: tokio_mpsc::UnboundedSender<WorkerCommand>,
}

enum WorkerCommand {
    Capture {
        reply: SyncSender<Result<Uuid, FocusError>>,
    },

    Restore {
        target: Uuid,
        reply: SyncSender<Result<(), FocusError>>,
    },

    IsActive {
        target: Uuid,
        reply: SyncSender<Result<bool, FocusError>>,
    },
}

#[derive(Debug)]
enum HelperResponse {
    Captured(String),

    CaptureUnavailable,

    RestoreRequested(String),

    RestoreNoTarget,

    RestoreNotFound(String),

    Active(String),

    ActiveNone,
}

struct FocusCallbackInterface {
    sender: tokio_mpsc::UnboundedSender<HelperResponse>,
}

#[zbus::interface(name = "io.github.riyanj220.PookiePaste.Focus")]
impl FocusCallbackInterface {
    #[zbus(name = "Captured")]
    fn captured(&self, id: String) {
        let _ = self.sender.send(HelperResponse::Captured(id));
    }

    #[zbus(name = "CaptureUnavailable")]
    fn capture_unavailable(&self) {
        let _ = self.sender.send(HelperResponse::CaptureUnavailable);
    }

    #[zbus(name = "RestoreRequested")]
    fn restore_requested(&self, id: String) {
        let _ = self.sender.send(HelperResponse::RestoreRequested(id));
    }

    #[zbus(name = "RestoreNoTarget")]
    fn restore_no_target(&self) {
        let _ = self.sender.send(HelperResponse::RestoreNoTarget);
    }

    #[zbus(name = "RestoreNotFound")]
    fn restore_not_found(&self, id: String) {
        let _ = self.sender.send(HelperResponse::RestoreNotFound(id));
    }

    #[zbus(name = "Active")]
    fn active(&self, id: String) {
        let _ = self.sender.send(HelperResponse::Active(id));
    }

    #[zbus(name = "ActiveNone")]
    fn active_none(&self) {
        let _ = self.sender.send(HelperResponse::ActiveNone);
    }
}

struct KdeFocusWorker {
    /*
     * Keeps our owned D-Bus name/object alive.
     */
    _connection: Connection,

    kglobalaccel: Proxy<'static>,

    responses: tokio_mpsc::UnboundedReceiver<HelperResponse>,
}

impl KdeFocusBackend {
    pub fn new() -> Result<Self, FocusError> {
        let (command_sender, command_receiver) = tokio_mpsc::unbounded_channel();

        let (init_sender, init_receiver) = mpsc::sync_channel(1);

        thread::Builder::new()
            .name("pookie-kde-focus".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,

                    Err(error) => {
                        let _ = init_sender.send(Err(FocusError::Failed(format!(
                            "failed to create KDE focus runtime: {error}"
                        ))));

                        return;
                    }
                };

                runtime.block_on(async move {
                    match KdeFocusWorker::new().await {
                        Ok(mut worker) => {
                            let _ = init_sender.send(Ok(()));

                            worker.run(command_receiver).await;
                        }

                        Err(error) => {
                            let _ = init_sender.send(Err(error));
                        }
                    }
                });
            })
            .map_err(|error| {
                FocusError::Failed(format!("failed to start KDE focus worker: {error}"))
            })?;

        match init_receiver.recv_timeout(WORKER_INIT_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                sender: command_sender,
            }),

            Ok(Err(error)) => Err(error),

            Err(error) => Err(FocusError::Failed(format!(
                "KDE focus worker initialization timed out or disconnected: {error}"
            ))),
        }
    }

    fn request<T>(
        &self,
        build_command: impl FnOnce(SyncSender<Result<T, FocusError>>) -> WorkerCommand,
    ) -> Result<T, FocusError> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);

        self.sender
            .send(build_command(reply_sender))
            .map_err(|_| FocusError::Failed("KDE focus worker is unavailable".to_string()))?;

        reply_receiver
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|error| {
                FocusError::Failed(format!(
                    "KDE focus command timed out or disconnected: {error}"
                ))
            })?
    }

    fn kde_target(target: FocusTarget) -> Result<Uuid, FocusError> {
        target.kde_id().ok_or_else(|| {
            FocusError::Failed(format!(
                "non-KDE focus target passed to KDE focus backend: {target}"
            ))
        })
    }
}

impl FocusBackend for KdeFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        let id = self.request(|reply| WorkerCommand::Capture { reply })?;

        Ok(FocusTarget::kde(id))
    }

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError> {
        let target = Self::kde_target(target)?;

        self.request(|reply| WorkerCommand::Restore { target, reply })
    }

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError> {
        let target = Self::kde_target(target)?;

        self.request(|reply| WorkerCommand::IsActive { target, reply })
    }
}

impl KdeFocusWorker {
    async fn new() -> Result<Self, FocusError> {
        let (response_sender, response_receiver) = tokio_mpsc::unbounded_channel();

        /*
         * Pookie owns one explicit D-Bus service/object.
         *
         * The KWin helper calls methods on this object
         * through callDBus(...).
         */
        let connection = zbus::connection::Builder::session()
            .map_err(|error| {
                FocusError::Failed(format!("failed to create KDE focus D-Bus builder: {error}"))
            })?
            .name(POOKIE_SERVICE)
            .map_err(|error| {
                FocusError::Failed(format!("failed to claim KDE focus D-Bus name: {error}"))
            })?
            .serve_at(
                POOKIE_FOCUS_PATH,
                FocusCallbackInterface {
                    sender: response_sender,
                },
            )
            .map_err(|error| {
                FocusError::Failed(format!("failed to expose KDE focus D-Bus object: {error}"))
            })?
            .build()
            .await
            .map_err(|error| {
                FocusError::Failed(format!(
                    "failed to connect KDE focus D-Bus service: {error}"
                ))
            })?;

        let kglobalaccel = Proxy::new_owned(
            connection.clone(),
            KGLOBALACCEL_SERVICE,
            KGLOBALACCEL_PATH,
            KGLOBALACCEL_INTERFACE,
        )
        .await
        .map_err(|error| {
            FocusError::Failed(format!("failed to create KGlobalAccel proxy: {error}"))
        })?;

        /*
         * Verify the installed helper contract.
         */
        let shortcut_names: Vec<String> =
            kglobalaccel
                .call("shortcutNames", &())
                .await
                .map_err(|error| {
                    FocusError::Failed(format!("failed to query KWin global shortcuts: {error}"))
                })?;

        for required in [ACTION_CAPTURE, ACTION_RESTORE, ACTION_ACTIVE] {
            if !shortcut_names.iter().any(|name| name == required) {
                return Err(FocusError::Unavailable);
            }
        }

        Ok(Self {
            _connection: connection,
            kglobalaccel,
            responses: response_receiver,
        })
    }

    async fn run(&mut self, mut receiver: tokio_mpsc::UnboundedReceiver<WorkerCommand>) {
        while let Some(command) = receiver.recv().await {
            match command {
                WorkerCommand::Capture { reply } => {
                    let result = self.capture().await;

                    let _ = reply.send(result);
                }

                WorkerCommand::Restore { target, reply } => {
                    let result = self.restore(target).await;

                    let _ = reply.send(result);
                }

                WorkerCommand::IsActive { target, reply } => {
                    let result = self.is_active(target).await;

                    let _ = reply.send(result);
                }
            }
        }
    }

    async fn invoke(&self, action: &str) -> Result<(), FocusError> {
        let _: () = self
            .kglobalaccel
            .call("invokeShortcut", &(action,))
            .await
            .map_err(|error| {
                FocusError::Failed(format!(
                    "failed to invoke KWin focus action {action}: {error}"
                ))
            })?;

        Ok(())
    }

    async fn next_response(&mut self) -> Result<HelperResponse, FocusError> {
        tokio::time::timeout(CALLBACK_TIMEOUT, self.responses.recv())
            .await
            .map_err(|_| {
                FocusError::Failed("timed out waiting for KWin focus helper callback".to_string())
            })?
            .ok_or_else(|| FocusError::Failed("KWin focus callback channel ended".to_string()))
    }

    async fn capture(&mut self) -> Result<Uuid, FocusError> {
        self.invoke(ACTION_CAPTURE).await?;

        loop {
            match self.next_response().await? {
                HelperResponse::Captured(value) => {
                    return Uuid::parse_str(&value).map_err(|error| {
                        FocusError::Failed(format!(
                            "invalid KWin window UUID returned by helper: {error}"
                        ))
                    });
                }

                HelperResponse::CaptureUnavailable => {
                    return Err(FocusError::Unavailable);
                }

                /*
                 * Ignore callbacks belonging to an older
                 * operation. The worker serializes commands,
                 * so these should normally not occur.
                 */
                _ => {}
            }
        }
    }

    async fn restore(&mut self, expected_target: Uuid) -> Result<(), FocusError> {
        self.invoke(ACTION_RESTORE).await?;

        loop {
            match self.next_response().await? {
                HelperResponse::RestoreRequested(value) => {
                    let restored_target = Uuid::parse_str(&value).map_err(|error| {
                        FocusError::Failed(format!("invalid restored KWin window UUID: {error}"))
                    })?;

                    if restored_target != expected_target {
                        return Err(FocusError::Failed(format!(
                            "KWin helper restored unexpected target: expected {expected_target}, got {restored_target}"
                        )));
                    }

                    return Ok(());
                }

                HelperResponse::RestoreNoTarget => {
                    return Err(FocusError::Unavailable);
                }

                HelperResponse::RestoreNotFound(value) => {
                    return Err(FocusError::Failed(format!(
                        "KWin focus target no longer exists: {value}"
                    )));
                }

                _ => {}
            }
        }
    }

    async fn is_active(&mut self, expected_target: Uuid) -> Result<bool, FocusError> {
        self.invoke(ACTION_ACTIVE).await?;

        loop {
            match self.next_response().await? {
                HelperResponse::Active(value) => {
                    let active_target = Uuid::parse_str(&value).map_err(|error| {
                        FocusError::Failed(format!("invalid active KWin window UUID: {error}"))
                    })?;

                    return Ok(active_target == expected_target);
                }

                HelperResponse::ActiveNone => {
                    return Ok(false);
                }

                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_action_ids_are_stable() {
        assert_eq!(ACTION_CAPTURE, "PookiePasteFocusCapture",);

        assert_eq!(ACTION_RESTORE, "PookiePasteFocusRestore",);

        assert_eq!(ACTION_ACTIVE, "PookiePasteFocusActive",);
    }

    #[test]
    fn final_dbus_contract_is_stable() {
        assert_eq!(POOKIE_SERVICE, "io.github.riyanj220.PookiePaste",);

        assert_eq!(POOKIE_FOCUS_PATH, "/io/github/riyanj220/PookiePaste/Focus",);
    }

    #[test]
    fn rejects_x11_target_for_kde_backend() {
        let result = KdeFocusBackend::kde_target(FocusTarget::x11(42));

        match result {
            Err(FocusError::Failed(message)) => {
                assert!(message.contains("non-KDE focus target",),);
            }

            _ => {
                panic!("expected KDE target mismatch failure");
            }
        }
    }

    #[test]
    fn extracts_kde_target_uuid() {
        let id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        let result = KdeFocusBackend::kde_target(FocusTarget::kde(id))
            .expect("KDE target should be accepted");

        assert_eq!(result, id);
    }
}
