//! SUMP3 ILA Integration Module
//!
//! Provides connection to sump-server for hardware ILA control and capture.

mod panel;
mod vcd;

pub use panel::draw_sump_panel;

use tracing::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;

use crate::async_util::perform_async_work;
use crate::message::Message;
use crate::EGUI_CONTEXT;

/// Request a repaint to wake up the UI after sending a message
fn request_repaint() {
    if let Ok(guard) = EGUI_CONTEXT.read() {
        if let Some(ctx) = guard.as_ref() {
            ctx.request_repaint();
        }
    }
}

pub use vcd::generate_vcd;

/// SUMP server connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// Main SUMP state container
pub struct SumpState {
    /// Server URL (e.g., "http://192.168.2.1:8082")
    pub server_url: String,
    /// Connection state
    pub connection: ConnectionState,
    /// ILA information from server
    pub ila_info: Option<IlaInfo>,
    /// Current capture status
    pub capture_status: Option<CaptureStatus>,
    /// Last captured data
    pub capture_data: Option<CaptureData>,
    /// Last captured source (hub, pod) for reload detection
    pub last_capture_source: Option<(u8, u8)>,
    /// Selected hub index
    pub selected_hub: u8,
    /// Selected pod index
    pub selected_pod: u8,
    /// Trigger configuration
    pub trigger_config: TriggerConfig,
    /// Number of samples to capture
    pub sample_count: u32,
    /// Auto-reload when capture completes
    pub auto_reload: bool,
    /// Polling for capture status
    pub polling_active: bool,
    /// UI state: show panel
    pub panel_visible: bool,
    /// URL input buffer for UI
    pub url_input: String,
    /// Trigger bits input buffer (hex string)
    pub trigger_bits_input: String,
    /// Embedded mode: served from sump-server, auto-connect to same origin
    pub embedded_mode: bool,
    /// Has auto-connect been attempted (only try once)
    pub auto_connect_attempted: bool,
}

impl Default for SumpState {
    fn default() -> Self {
        // Detect embedded mode: if served from sump-server (port 8082), auto-connect
        let (embedded_mode, url_input) = detect_embedded_mode();
        
        Self {
            server_url: String::new(),
            connection: ConnectionState::Disconnected,
            ila_info: None,
            capture_status: None,
            capture_data: None,
            last_capture_source: None,
            selected_hub: 0,
            selected_pod: 0,
            trigger_config: TriggerConfig::default(),
            sample_count: 2048,
            auto_reload: true,  // Always auto-load waveforms
            polling_active: false,
            panel_visible: true,
            url_input,
            trigger_bits_input: "00000001".to_string(),
            embedded_mode,
            auto_connect_attempted: false,
        }
    }
}

/// Detect if we're running in embedded mode (served from sump-server)
fn detect_embedded_mode() -> (bool, String) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(origin) = window.location().origin() {
                // If served from sump-server (port 8082), we're in embedded mode
                if origin.contains(":8082") {
                    return (true, origin);
                }
            }
        }
    }
    (false, String::new())
}

impl SumpState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if we're connected to a server
    pub fn is_connected(&self) -> bool {
        matches!(self.connection, ConnectionState::Connected)
    }

    /// Get the currently selected hub info
    pub fn selected_hub_info(&self) -> Option<&HubInfo> {
        self.ila_info
            .as_ref()
            .and_then(|info| info.hubs.iter().find(|h| h.index == self.selected_hub))
    }

    /// Get the currently selected pod info
    pub fn selected_pod_info(&self) -> Option<&PodInfo> {
        self.selected_hub_info()
            .and_then(|hub| hub.pods.iter().find(|p| p.index == self.selected_pod))
    }

    /// Get list of available hub/pod combinations
    pub fn available_sources(&self) -> Vec<(u8, u8, String)> {
        let mut sources = Vec::new();
        if let Some(info) = &self.ila_info {
            for hub in &info.hubs {
                for pod in &hub.pods {
                    let name = format!(
                        "Hub {} / Pod {} ({})",
                        hub.index,
                        pod.index,
                        hub.name.trim()
                    );
                    sources.push((hub.index, pod.index, name));
                }
            }
        }
        sources
    }
}

// ============================================================================
// API Data Types (matching sump-server responses)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IlaInfo {
    pub connected: bool,
    pub hw_id: String,
    pub revision: u8,
    pub hub_count: u8,
    pub is_armed: bool,
    pub is_awake: bool,
    pub base_addr: String,
    pub hubs: Vec<HubInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubInfo {
    pub index: u8,
    pub name: String,
    pub freq_mhz: u32,
    pub pod_count: u8,
    pub pods: Vec<PodInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodInfo {
    pub index: u8,
    pub name: String,
    pub hw_rev: u8,
    pub ram_depth: u32,
    pub data_bits: u16,
    pub ts_bits: u8,
    pub triggerable: u32,
    pub rle_disable: bool,
    pub view_rom_en: bool,
    pub view_mode: String,
    pub signals: Vec<SignalInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalInfo {
    pub name: String,
    pub bit_high: u16,
    pub bit_low: u16,
    pub signal_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureStatus {
    pub armed: bool,
    pub pre_trigger: bool,
    pub triggered: bool,
    pub acquired: bool,
    pub init_in_progress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureData {
    pub hub: u8,
    pub pod: u8,
    pub ts_bits: u8,
    pub data_bits: u16,
    pub status: CaptureStatus,
    pub samples: Vec<RleSample>,
    pub sample_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RleSample {
    pub address: u32,
    pub code: u8,
    pub timestamp: u32,
    pub data: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    #[serde(default)]
    pub trigger_type: String,
    #[serde(default)]
    pub trigger_bits: u32,
    #[serde(default = "default_post_trigger")]
    pub post_trigger: u32,
}

fn default_post_trigger() -> u32 {
    64
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            trigger_type: "or_rising".to_string(),
            trigger_bits: 0x00000001,
            post_trigger: 64,
        }
    }
}

impl TriggerConfig {
    pub fn new_or_rising(bits: u32, post_trigger: u32) -> Self {
        Self {
            trigger_type: "or_rising".to_string(),
            trigger_bits: bits,
            post_trigger,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
}

// ============================================================================
// API Client Functions
// ============================================================================

/// Fetch ILA info from the server
pub fn fetch_ila_info(server_url: String, sender: Sender<Message>) {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/api/ila", base);
    let task = async move {
        let result = reqwest::get(&url).await;
        match result {
            Ok(response) => match response.json::<IlaInfo>().await {
                Ok(info) => {
                    info!("SUMP: Connected to {} - {} hubs", server_url, info.hub_count);
                    sender
                        .send(Message::SumpInfoReceived(server_url, info))
                        .ok();
                    request_repaint();
                }
                Err(e) => {
                    error!("SUMP: Failed to parse ILA info: {}", e);
                    sender
                        .send(Message::SumpConnectionError(format!(
                            "Failed to parse response: {}",
                            e
                        )))
                        .ok();
                    request_repaint();
                }
            },
            Err(e) => {
                error!("SUMP: Connection failed: {}", e);
                sender
                    .send(Message::SumpConnectionError(format!(
                        "Connection failed: {}",
                        e
                    )))
                    .ok();
            }
        }
    };

    perform_async_work(task);
}

/// Fetch capture status from the server
pub fn fetch_capture_status(server_url: String, sender: Sender<Message>) {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/api/ila/status", base);
    let task = async move {
        if let Ok(response) = reqwest::get(&url).await {
            if let Ok(status) = response.json::<CaptureStatus>().await {
                sender.send(Message::SumpStatusReceived(status)).ok();
            }
        }
    };

    perform_async_work(task);
}

/// Configure trigger and arm the ILA
pub fn configure_and_arm(server_url: String, config: TriggerConfig, sender: Sender<Message>) {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/api/ila/trigger", base);
    let task = async move {
        let client = reqwest::Client::new();
        match client.post(&url).json(&config).send().await {
            Ok(response) => match response.json::<CommandResult>().await {
                Ok(result) => {
                    if result.success {
                        info!("SUMP: Armed - {}", result.message);
                        sender.send(Message::SumpCommandOk("Armed".to_string())).ok();
                    } else {
                        warn!("SUMP: Arm failed - {}", result.message);
                        sender
                            .send(Message::SumpCommandError(result.message))
                            .ok();
                    }
                }
                Err(e) => {
                    sender
                        .send(Message::SumpCommandError(format!("Parse error: {}", e)))
                        .ok();
                }
            },
            Err(e) => {
                sender
                    .send(Message::SumpCommandError(format!("Request failed: {}", e)))
                    .ok();
            }
        }
    };

    perform_async_work(task);
}

/// Capture data from the ILA
pub fn capture_data(
    server_url: String,
    hub: u8,
    pod: u8,
    count: u32,
    sender: Sender<Message>,
) {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/api/ila/capture/{}/{}/{}", base, hub, pod, count);
    let task = async move {
        match reqwest::get(&url).await {
            Ok(response) => match response.json::<CaptureData>().await {
                Ok(data) => {
                    info!(
                        "SUMP: Captured {} samples from hub {}, pod {}",
                        data.samples.len(),
                        hub,
                        pod
                    );
                    sender.send(Message::SumpCaptureReceived(data)).ok();
                }
                Err(e) => {
                    sender
                        .send(Message::SumpCommandError(format!(
                            "Failed to parse capture: {}",
                            e
                        )))
                        .ok();
                }
            },
            Err(e) => {
                sender
                    .send(Message::SumpCommandError(format!("Capture failed: {}", e)))
                    .ok();
            }
        }
    };

    perform_async_work(task);
}

/// Send reset command
pub fn send_reset(server_url: String, sender: Sender<Message>) {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/api/ila/reset", base);
    let task = async move {
        let client = reqwest::Client::new();
        if let Ok(response) = client.post(&url).send().await {
            if let Ok(result) = response.json::<CommandResult>().await {
                if result.success {
                    info!("SUMP: Reset complete");
                    sender.send(Message::SumpCommandOk("Reset complete".to_string())).ok();
                } else {
                    sender.send(Message::SumpCommandError(result.message)).ok();
                }
            }
        }
    };

    perform_async_work(task);
}

/// Send init command
pub fn send_init(server_url: String, sender: Sender<Message>) {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/api/ila/init", base);
    let task = async move {
        let client = reqwest::Client::new();
        if let Ok(response) = client.post(&url).send().await {
            if let Ok(result) = response.json::<CommandResult>().await {
                if result.success {
                    info!("SUMP: Init complete");
                    sender.send(Message::SumpCommandOk("Init complete".to_string())).ok();
                } else {
                    sender.send(Message::SumpCommandError(result.message)).ok();
                }
            }
        }
    };

    perform_async_work(task);
}

/// Configure trigger, arm, and capture in one async sequence
pub fn configure_arm_and_capture(
    server_url: String,
    config: TriggerConfig,
    hub: u8,
    pod: u8,
    count: u32,
    sender: Sender<Message>,
) {
    let base = server_url.trim_end_matches('/').to_string();
    info!("SUMP: Starting capture sequence...");
    let task = async move {
        let client = reqwest::Client::new();

        // Step 1: Configure trigger and arm
        let trigger_url = format!("{}/api/ila/trigger", base);
        info!("SUMP: [T+0ms] Sending trigger config: type={}, bits=0x{:08x}, post={}", 
              config.trigger_type, config.trigger_bits, config.post_trigger);

        match client.post(&trigger_url).json(&config).send().await {
            Ok(response) => {
                match response.json::<CommandResult>().await {
                    Ok(result) => {
                        if !result.success {
                            sender
                                .send(Message::SumpCommandError(format!("Arm failed: {}", result.message)))
                                .ok();
                            return;
                        }
                    }
                    Err(e) => {
                        sender
                            .send(Message::SumpCommandError(format!("Parse error: {}", e)))
                            .ok();
                        return;
                    }
                }
            }
            Err(e) => {
                sender
                    .send(Message::SumpCommandError(format!("Trigger request failed: {}", e)))
                    .ok();
                return;
            }
        }

        // Step 2: Capture data
        info!("SUMP: [T+?ms] Trigger done, starting capture...");
        let capture_url = format!("{}/api/ila/capture/{}/{}/{}", base, hub, pod, count);
        match reqwest::get(&capture_url).await {
            Ok(response) => {
                info!("SUMP: [T+?ms] Got capture response, parsing JSON...");
                match response.json::<CaptureData>().await {
                    Ok(data) => {
                        info!(
                            "SUMP: Captured {} samples from hub {}, pod {}, sending message",
                            data.samples.len(),
                            hub,
                            pod
                        );
                        sender.send(Message::SumpCaptureReceived(data)).ok();
                        request_repaint();
                        info!("SUMP: Message sent, repaint requested");
                    }
                    Err(e) => {
                        sender
                            .send(Message::SumpCommandError(format!(
                                "Failed to parse capture: {}",
                                e
                            )))
                            .ok();
                    }
                }
            }
            Err(e) => {
                sender
                    .send(Message::SumpCommandError(format!("Capture failed: {}", e)))
                    .ok();
            }
        }
    };

    perform_async_work(task);
}
