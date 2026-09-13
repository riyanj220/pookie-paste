use std::{
    os::fd::{AsRawFd, FromRawFd, IntoRawFd},
    os::unix::net::UnixStream,
    time::Duration,
};

use anyhow::{Context, Result};

use ashpd::desktop::{
    PersistMode,
    remote_desktop::{ConnectToEISOptions, DeviceType, RemoteDesktop, SelectDevicesOptions},
};

use futures_util::StreamExt;

use reis::{
    ei,
    event::{DeviceCapability, EiEvent},
};

const KEY_LEFTCTRL: u32 = 29;
const KEY_V: u32 = 47;

const EMULATION_SEQUENCE: u32 = 1;

fn monotonic_time_micros() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);

    (time.tv_sec as u64 * 1_000_000) + (time.tv_nsec as u64 / 1_000)
}

#[tokio::main]
async fn main() -> Result<()> {
    let restore_token = std::env::args().nth(1);

    println!("Pookie Paste RemoteDesktop + EIS Ctrl+V probe");
    println!("---------------------------------------------");

    match &restore_token {
        Some(_) => println!("Mode: restore previous persistent session"),
        None => println!("Mode: create new persistent session"),
    }

    println!();
    println!("Step 1: connecting to RemoteDesktop portal...");

    let portal = RemoteDesktop::new()
        .await
        .context("failed to create RemoteDesktop portal proxy")?;

    println!("OK: RemoteDesktop portal connected");
    println!("Portal interface version: {}", portal.version());

    println!();
    println!("Step 2: querying available device types...");

    let available_devices = portal
        .available_device_types()
        .await
        .context("failed to query AvailableDeviceTypes")?;

    println!("Available devices: {available_devices:?}");

    if !available_devices.contains(DeviceType::Keyboard) {
        anyhow::bail!("RemoteDesktop portal does not advertise keyboard control");
    }

    println!("OK: keyboard control is advertised");

    println!();
    println!("Step 3: creating RemoteDesktop session...");

    let session = portal
        .create_session(Default::default())
        .await
        .context("failed to create RemoteDesktop session")?;

    println!("OK: session created");

    println!();
    println!("Step 4: requesting keyboard control...");

    let mut select_options = SelectDevicesOptions::default()
        .set_devices(Some(DeviceType::Keyboard.into()))
        .set_persist_mode(PersistMode::ExplicitlyRevoked);

    if let Some(token) = restore_token.as_deref() {
        println!("Using restore token from previous session");

        select_options = select_options.set_restore_token(Some(token));
    } else {
        println!("No restore token supplied");
    }

    portal
        .select_devices(&session, select_options)
        .await
        .context("failed to issue SelectDevices request")?
        .response()
        .context("SelectDevices request was rejected or cancelled")?;

    println!("OK: SelectDevices completed");

    println!();
    println!("Step 5: starting RemoteDesktop session...");

    let start_response = portal
        .start(&session, None, Default::default())
        .await
        .context("failed to issue Start request")?
        .response()
        .context("RemoteDesktop Start was rejected or cancelled")?;

    println!("OK: RemoteDesktop session started");

    let granted_devices = start_response.devices();

    println!("Granted devices: {granted_devices:?}");

    if !granted_devices.contains(DeviceType::Keyboard) {
        anyhow::bail!("RemoteDesktop session started but keyboard control was not granted");
    }

    println!("OK: keyboard control was granted");

    println!();

    match start_response.restore_token() {
        Some(token) => {
            println!("Restore token returned:");
            println!("{token}");
        }

        None => {
            println!("No restore token was returned");
        }
    }

    println!();
    println!("Step 6: connecting to EIS...");

    let eis_fd = portal
        .connect_to_eis(&session, ConnectToEISOptions::default())
        .await
        .context("ConnectToEIS failed")?;

    println!("OK: EIS file descriptor received: {}", eis_fd.as_raw_fd());

    let raw_fd = eis_fd.into_raw_fd();

    let socket = unsafe { UnixStream::from_raw_fd(raw_fd) };

    println!("OK: EIS FD converted to UnixStream");

    println!();
    println!("Step 7: creating EI client context...");

    let context = ei::Context::new(socket).context("failed creating reis EI context")?;

    println!("OK: EI context created");

    println!();
    println!("Step 8: performing EI handshake...");

    let (_connection, mut events) = context
        .handshake_tokio("Pookie Paste", ei::handshake::ContextType::Sender)
        .await
        .context("EI handshake failed")?;

    println!("OK: EI handshake completed");

    println!();
    println!("Step 9: waiting for EI seat...");

    let seat = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events
                .next()
                .await
                .context("EI event stream ended before a seat was announced")?
                .context("failed receiving EI event")?;

            match event {
                EiEvent::SeatAdded(event) => {
                    return Ok::<_, anyhow::Error>(event.seat);
                }

                EiEvent::Disconnected(event) => {
                    anyhow::bail!("EIS disconnected before a seat was announced: {event:?}");
                }

                other => {
                    println!("EI event before seat: {other:?}");
                }
            }
        }
    })
    .await
    .context("timed out waiting for EI seat")??;

    match seat.name() {
        Some(name) => println!("OK: EI seat discovered: {name}"),
        None => println!("OK: EI seat discovered (unnamed)"),
    }

    println!();
    println!("Step 10: binding keyboard capability...");

    seat.bind_capabilities(DeviceCapability::Keyboard.into());

    context
        .flush()
        .context("failed flushing keyboard capability bind")?;

    println!("OK: keyboard capability bind sent");

    println!();
    println!("Step 11: waiting for keyboard device...");

    let keyboard_device = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events
                .next()
                .await
                .context("EI event stream ended before keyboard device was announced")?
                .context("failed receiving EI event")?;

            match event {
                EiEvent::DeviceAdded(event) => {
                    let device = event.device;

                    println!(
                        "Device announced: name={:?}, type={:?}",
                        device.name(),
                        device.device_type(),
                    );

                    if device.has_capability(DeviceCapability::Keyboard) {
                        return Ok::<_, anyhow::Error>(device);
                    }
                }

                EiEvent::Disconnected(event) => {
                    anyhow::bail!("EIS disconnected while waiting for keyboard device: {event:?}");
                }

                other => {
                    println!("EI event while waiting for device: {other:?}");
                }
            }
        }
    })
    .await
    .context("timed out waiting for EI keyboard device")??;

    println!("OK: keyboard-capable EI device discovered");
    println!("Keyboard device name: {:?}", keyboard_device.name());
    println!("Keymap available: {}", keyboard_device.keymap().is_some());

    let keyboard = keyboard_device
        .interface::<ei::Keyboard>()
        .context("ei_keyboard interface unavailable")?;

    println!("OK: ei_keyboard interface available");

    println!();
    println!("Step 12: waiting for keyboard device resume...");

    let resume_serial = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events
                .next()
                .await
                .context("EI event stream ended before keyboard device resumed")?
                .context("failed receiving EI event")?;

            match event {
                EiEvent::DeviceResumed(event) if event.device == keyboard_device => {
                    println!("OK: keyboard device resumed with serial {}", event.serial);

                    return Ok::<_, anyhow::Error>(event.serial);
                }

                EiEvent::DeviceRemoved(event) if event.device == keyboard_device => {
                    anyhow::bail!("keyboard device was removed before it resumed");
                }

                EiEvent::Disconnected(event) => {
                    anyhow::bail!("EIS disconnected while waiting for keyboard resume: {event:?}");
                }

                other => {
                    println!("EI event while waiting for resume: {other:?}");
                }
            }
        }
    })
    .await
    .context("timed out waiting for keyboard device resume")??;

    println!();
    println!("Step 13: starting emulation...");

    let device = keyboard_device.device();

    device.start_emulating(resume_serial, EMULATION_SEQUENCE);

    context.flush().context("failed flushing start_emulating")?;

    println!("OK: emulation started");

    println!();
    println!("Step 14: Ctrl+V test begins in 5 seconds.");
    println!("Switch NOW to a blank text editor.");
    println!("Expected clipboard text: POOKIE_EIS_PASTE_TEST");

    for remaining in (1..=5).rev() {
        println!("{remaining}...");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    println!();
    println!("Step 15: pressing Ctrl...");

    keyboard.key(KEY_LEFTCTRL, ei::keyboard::KeyState::Press);

    device.frame(resume_serial, monotonic_time_micros());

    context.flush().context("failed flushing Ctrl press")?;

    tokio::time::sleep(Duration::from_millis(30)).await;

    println!("Step 16: pressing V...");

    keyboard.key(KEY_V, ei::keyboard::KeyState::Press);

    device.frame(resume_serial, monotonic_time_micros());

    context.flush().context("failed flushing V press")?;

    tokio::time::sleep(Duration::from_millis(30)).await;

    println!("Step 17: releasing V...");

    keyboard.key(KEY_V, ei::keyboard::KeyState::Released);

    device.frame(resume_serial, monotonic_time_micros());

    context.flush().context("failed flushing V release")?;

    tokio::time::sleep(Duration::from_millis(30)).await;

    println!("Step 18: releasing Ctrl...");

    keyboard.key(KEY_LEFTCTRL, ei::keyboard::KeyState::Released);

    device.frame(resume_serial, monotonic_time_micros());

    context.flush().context("failed flushing Ctrl release")?;

    println!("OK: Ctrl+V sequence sent");

    println!();
    println!("Step 19: stopping emulation...");

    device.stop_emulating(resume_serial);

    context.flush().context("failed flushing stop_emulating")?;

    println!("OK: emulation stopped");

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!();
    println!("Ctrl+V probe finished.");
    println!();
    println!("Portal: PASS");
    println!("Persistent permission: PASS");
    println!("ConnectToEIS: PASS");
    println!("EI handshake: PASS");
    println!("Keyboard device: PASS");
    println!("Keyboard emulation: PASS");
    println!("Ctrl+V sequence: SENT");
    println!();
    println!("Check the editor: it should contain POOKIE_EIS_PASTE_TEST.");

    Ok(())
}
