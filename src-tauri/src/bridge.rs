//! The test bridge (`--bridge-url mqtt://127.0.0.1:PORT`): the app publishes what
//! it does to a local MQTT broker and obeys the commands a test publishes there.
//! Topics, payloads and the command allowlist are `resonance_core::bridge`; this
//! file only moves them.
//!
//! * every backend -> UI event (`EVENT_NAMES`) is heard with `listen_any` and
//!   published as `rs/app/event/<name>`, so no emit site changed;
//! * `rs/test/command/<name>` is parsed, run, and answered on `rs/app/ack/<id>`.
//!
//! A bridge that is slow or missing never blocks the app: publishing only queues
//! (a full queue drops the message, with a warning), and the connection thread
//! keeps retrying. Honoured only when the test flags are (`test_env::GATE_OPEN`);
//! the payloads hold chat text, which is why the address must be loopback.

use resonance_core::bridge::{self as wire, Command, EVENT_NAMES};
use rumqttc::{Client, Event, LastWill, MqttOptions, Packet, QoS};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::ipc::{Invoke, InvokeBody};
use tauri::{AppHandle, Listener, Runtime};

/// The connection, once the bridge is on; the events and the command tap share it.
static CLIENT: OnceLock<Client> = OnceLock::new();
/// One counter for everything published, so a test sees the order of events and commands.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Messages that can wait for the connection thread before new ones are dropped.
const QUEUE: usize = 1024;
const RETRY: Duration = Duration::from_secs(1);

/// Starts the bridge when `--bridge-url` was given; does nothing otherwise.
pub fn start(app: &AppHandle) {
    let Some((host, port)) = crate::test_env::bridge_addr() else {
        return;
    };
    // rumqttc wants the bare address of an IPv6 host.
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let mut options = MqttOptions::new("resonance-stream-app", &host, port);
    options.set_keep_alive(Duration::from_secs(5));
    options.set_last_will(LastWill::new(
        wire::STATUS_TOPIC,
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (client, mut connection) = Client::new(options, QUEUE);

    let _ = CLIENT.set(client.clone());

    // Events: heard on any target, published in the order they arrive.
    for name in EVENT_NAMES {
        app.listen_any(name, move |event| {
            let payload = serde_json::from_str(event.payload())
                .unwrap_or_else(|_| Value::String(event.payload().to_string()));
            publish_envelope(wire::event_topic(name), name, payload);
        });
    }

    let app = app.clone();
    std::thread::spawn(move || {
        log::info!("[Bridge] Using the broker at {host}:{port}");
        for notification in connection.iter() {
            match notification {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    log::info!("[Bridge] Connected");
                    let _ =
                        client.try_publish(wire::STATUS_TOPIC, QoS::AtLeastOnce, true, "online");
                    let _ = client.try_subscribe(wire::COMMAND_FILTER, QoS::AtLeastOnce);
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    handle(&app, &client, &publish.topic, &publish.payload);
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("[Bridge] {e}");
                    std::thread::sleep(RETRY);
                }
            }
        }
    });
}

/// Wraps the app's command handler: every UI -> backend `invoke` is published
/// as `rs/app/command/<name>` (with its arguments) before it runs. Does nothing
/// else, and nothing at all when the bridge is off.
pub fn tap_commands<R: Runtime>(
    inner: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if CLIENT.get().is_some() {
            let name = invoke.message.command();
            if let Some(topic) = wire::command_topic(name) {
                let payload = match invoke.message.payload() {
                    InvokeBody::Json(json) => json.clone(),
                    InvokeBody::Raw(bytes) => serde_json::json!({ "raw_bytes": bytes.len() }),
                };
                publish_envelope(topic, name, payload);
            }
        }
        inner(invoke)
    }
}

/// Queues one enveloped message; a full queue drops it, with a warning.
fn publish_envelope(topic: String, name: &str, payload: Value) {
    let Some(client) = CLIENT.get() else { return };
    let text = wire::envelope(SEQ.fetch_add(1, Ordering::SeqCst), now_ms(), name, payload);
    if let Err(e) = client.try_publish(topic, QoS::AtLeastOnce, false, text.into_bytes()) {
        log::warn!("[Bridge] Dropped {name}: {e}");
    }
}

/// Runs one command and answers it.
fn handle(app: &AppHandle, client: &Client, topic: &str, payload: &[u8]) {
    let publish = |topic: String, text: String| {
        if let Err(e) = client.try_publish(topic, QoS::AtLeastOnce, false, text.into_bytes()) {
            log::warn!("[Bridge] Dropped an answer: {e}");
        }
    };
    let request = match wire::parse_command(topic, payload) {
        Ok(request) => request,
        Err(e) => {
            log::warn!("[Bridge] Refused {topic}: {e}");
            let answer = match e.id() {
                Some(id) => (wire::ack_topic(id), wire::ack(id, Err(&e.to_string()))),
                None => (wire::ERROR_TOPIC.to_string(), wire::error_message(&e)),
            };
            return publish(answer.0, answer.1);
        }
    };
    log::info!("[Bridge] Command {:?}", request.command);
    match &request.command {
        Command::Ping => {}
        Command::ReplayChat { path } => {
            // Reports its own problems as system messages (what `--replay-chat` does).
            crate::services::sniffer::replay::start_file(app.clone(), path);
        }
        Command::Quit => {
            publish(wire::ack_topic(&request.id), wire::ack(&request.id, Ok(())));
            // Let the ack leave before the process does.
            std::thread::sleep(Duration::from_millis(200));
            app.exit(0);
            return;
        }
    }
    publish(wire::ack_topic(&request.id), wire::ack(&request.id, Ok(())));
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
