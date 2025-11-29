use std::sync::mpsc::Sender;
use std::sync::Arc;

use bincode::Options;
use eyre::{anyhow, Context, Result};
use eyre::{bail, eyre};
use reqwest::StatusCode;
use thiserror::Error;
use tracing::{error, info, warn};
use wellen::CompressedTimeTable;

use surver::{
    SurverStatus, BINCODE_OPTIONS, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, SURFER_VERSION,
    WELLEN_VERSION, X_SURFER_VERSION, X_WELLEN_VERSION,
};

use super::HierarchyResponse;
use crate::async_util::sleep_ms;
use crate::message::Message;
use crate::spawn;
use crate::wave_source::{LoadOptions, WaveSource};
use crate::wellen::{BodyResult, HeaderResult};

#[derive(Debug, Error)]
pub enum ReloadError {
    #[error("Reload requested too frequently, please wait before trying again")]
    TooFrequent,
    #[error("File unchanged since last reload")]
    FileUnchanged,
    #[error("Unexpected response code: {0}")]
    UnexpectedStatus(StatusCode),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("Response validation error: {0}")]
    Validation(#[from] eyre::Report),
}

fn check_response(server_url: &str, response: &reqwest::Response) -> Result<()> {
    let server = response
        .headers()
        .get(HTTP_SERVER_KEY)
        .ok_or(eyre!("no server header"))?
        .to_str()?;
    if server != HTTP_SERVER_VALUE_SURFER {
        bail!("Unexpected server {server} from {server_url}");
    }
    let surfer_version = response
        .headers()
        .get(X_SURFER_VERSION)
        .ok_or(eyre!("no surfer version header"))?
        .to_str()?;
    if surfer_version != SURFER_VERSION {
        // this mismatch may be OK as long as the wellen version matches
        info!(
            "Surfer version on the server: {surfer_version} does not match client version {SURFER_VERSION}"
        );
    }
    let wellen_version = response
        .headers()
        .get(X_WELLEN_VERSION)
        .ok_or(eyre!("no wellen version header"))?
        .to_str()?;
    if wellen_version != WELLEN_VERSION {
        bail!(
            "Version incompatibility! The server uses wellen {wellen_version}, our client uses wellen {WELLEN_VERSION}"
        );
    }
    Ok(())
}

async fn get_status(server: String) -> Result<SurverStatus> {
    let client = reqwest::Client::new();
    let response = client.get(format!("{server}/get_status")).send().await?;
    check_response(&server, &response)?;
    let body = response.text().await?;
    let status = serde_json::from_str::<SurverStatus>(&body)?;
    Ok(status)
}

async fn reload(server: String) -> std::result::Result<SurverStatus, ReloadError> {
    let client = reqwest::Client::new();
    let response = client.get(format!("{server}/reload")).send().await?;
    check_response(&server, &response)?;
    let status_code = response.status();
    let body = response.text().await?;
    match status_code {
        StatusCode::TOO_MANY_REQUESTS => {
            info!("Reload too frequent");
            Err(ReloadError::TooFrequent)
        }
        StatusCode::NOT_MODIFIED => {
            info!("File unchanged");
            Err(ReloadError::FileUnchanged)
        }
        StatusCode::ACCEPTED => {
            info!("File reloaded at server");
            let status = serde_json::from_str::<SurverStatus>(&body)?;
            Ok(status)
        }
        code => {
            warn!("Unexpected response code: {code}");
            Err(ReloadError::UnexpectedStatus(code))
        }
    }
}

async fn get_hierarchy(server: String) -> Result<HierarchyResponse> {
    let client = reqwest::Client::new();
    let response = client.get(format!("{server}/get_hierarchy")).send().await?;
    check_response(&server, &response)?;
    let compressed = response.bytes().await?;
    let raw = lz4_flex::decompress_size_prepended(&compressed)?;
    let mut reader = std::io::Cursor::new(raw);
    // first we read a value, expecting there to be more bytes
    let opts = BINCODE_OPTIONS.allow_trailing_bytes();
    let file_format: wellen::FileFormat = opts.deserialize_from(&mut reader)?;
    // the last value should consume all remaining bytes
    let hierarchy: wellen::Hierarchy = BINCODE_OPTIONS.deserialize_from(&mut reader)?;
    Ok(HierarchyResponse {
        hierarchy,
        file_format,
    })
}

async fn get_time_table(server: String) -> Result<Vec<wellen::Time>> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{server}/get_time_table"))
        .send()
        .await?;
    check_response(&server, &response)?;
    let compressed_data = response.bytes().await?;
    let compressed: CompressedTimeTable = BINCODE_OPTIONS.deserialize(&compressed_data)?;
    let table = compressed.uncompress();
    Ok(table)
}

pub async fn get_signals(
    server: String,
    signals: &[wellen::SignalRef],
) -> Result<Vec<(wellen::SignalRef, wellen::Signal)>> {
    let client = reqwest::Client::new();
    let mut url = format!("{server}/get_signals");
    for signal in signals.iter() {
        url.push_str(&format!("/{}", signal.index()));
    }

    let response = client.get(url).send().await?;
    check_response(&server, &response)?;
    let data = response.bytes().await?;
    let mut reader = std::io::Cursor::new(data);
    let num_ids: u64 = leb128::read::unsigned(&mut reader)?;
    if num_ids > signals.len() as u64 {
        bail!(
            "Too many signals in response: {num_ids}, expected {}",
            signals.len()
        );
    }
    if num_ids == 0 {
        return Ok(vec![]);
    }

    let opts = BINCODE_OPTIONS.allow_trailing_bytes();
    let mut out = Vec::with_capacity(num_ids as usize);
    for _ in 0..(num_ids - 1) {
        let compressed: wellen::CompressedSignal = opts.deserialize_from(&mut reader)?;
        let signal = compressed.uncompress();
        out.push((signal.signal_ref(), signal));
    }
    // for the final signal, we expect to consume all bytes
    let compressed: wellen::CompressedSignal = BINCODE_OPTIONS.deserialize_from(&mut reader)?;
    let signal = compressed.uncompress();
    out.push((signal.signal_ref(), signal));
    Ok(out)
}

pub fn get_hierarchy_from_server(
    sender: Sender<Message>,
    server: String,
    load_options: LoadOptions,
) {
    let start = web_time::Instant::now();
    let source = WaveSource::Url(server.clone());

    let task = async move {
        let res = get_hierarchy(server.clone())
            .await
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to retrieve hierarchy from remote server {server}"));

        let msg = match res {
            Ok(h) => {
                let header = HeaderResult::Remote(Arc::new(h.hierarchy), h.file_format, server);
                Message::WaveHeaderLoaded(start, source, load_options, header)
            }
            Err(e) => Message::Error(e),
        };
        if let Err(e) = sender.send(msg) {
            error!("Failed to send message: {e}");
        }
    };
    spawn!(task);
}

pub fn get_time_table_from_server(sender: Sender<Message>, server: String) {
    let start = web_time::Instant::now();
    let source = WaveSource::Url(server.clone());

    let task = async move {
        let res = get_time_table(server.clone())
            .await
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to retrieve time table from remote server {server}"));

        let msg = match res {
            Ok(table) => Message::WaveBodyLoaded(start, source, BodyResult::Remote(table, server)),
            Err(e) => Message::Error(e),
        };
        if let Err(e) = sender.send(msg) {
            error!("Failed to send message: {e}");
        }
    };
    spawn!(task);
}

pub fn get_server_status(sender: Sender<Message>, server: String, delay_ms: u64) {
    let start = web_time::Instant::now();
    let task = async move {
        sleep_ms(delay_ms).await;
        let res = get_status(server.clone())
            .await
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to retrieve status from remote server {server}"));

        let msg = match res {
            Ok(status) => Message::SurferServerStatus(start, server, status),
            Err(e) => Message::Error(e),
        };
        if let Err(e) = sender.send(msg) {
            error!("Failed to send message: {e}");
        }
    };
    spawn!(task);
}

pub fn server_reload(sender: Sender<Message>, server: String, delay_ms: u64) {
    let start = web_time::Instant::now();
    let task = async move {
        sleep_ms(delay_ms).await;
        let res = reload(server.clone()).await;

        let msg = match res {
            Ok(status) => Message::SurferServerStatus(start, server, status),
            Err(crate::remote::ReloadError::TooFrequent) => {
                info!("Reload request was rate-limited by server");
                return; // Don't send error message for expected rate limiting
            }
            Err(crate::remote::ReloadError::FileUnchanged) => {
                info!("File unchanged, no reload needed");
                return; // Don't send error message for unchanged file
            }
            Err(e) => {
                let err = anyhow!("{e:?}");
                Message::Error(err)
            }
        };
        if let Err(e) = sender.send(msg) {
            error!("Failed to send message: {e}");
        }
    };
    spawn!(task);
}
