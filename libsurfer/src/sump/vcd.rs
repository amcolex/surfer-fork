//! VCD Generation from SUMP capture data
//!
//! Converts RLE-encoded capture samples to VCD format for loading into surfer.

use super::{CaptureData, HubInfo, PodInfo, SignalInfo};
use std::fmt::Write;

/// Generate VCD data from a SUMP capture
pub fn generate_vcd(
    capture: &CaptureData,
    hub_info: Option<&HubInfo>,
    pod_info: Option<&PodInfo>,
) -> Vec<u8> {
    let mut vcd = String::new();

    // Get configuration from hub/pod info or use defaults
    let freq_mhz = hub_info.map(|h| h.freq_mhz).unwrap_or(100);
    let hub_name = hub_info
        .map(|h| h.name.trim().to_string())
        .unwrap_or_else(|| format!("hub{}", capture.hub));
    let pod_name = pod_info
        .map(|p| p.name.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("pod{}", capture.pod));
    let period_ns = 1000 / freq_mhz.max(1);

    // Clean scope name for VCD (alphanumeric and underscore only)
    let scope_name = clean_signal_name(&pod_name);

    // Get signal definitions
    let signals: Vec<SignalInfo> = pod_info
        .map(|p| p.signals.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // Default: single data signal with full width
            vec![SignalInfo {
                name: format!("data[{}:0]", capture.data_bits.saturating_sub(1)),
                bit_high: capture.data_bits.saturating_sub(1),
                bit_low: 0,
                signal_type: "vector".to_string(),
            }]
        });

    // VCD Header
    writeln!(vcd, "$date").ok();
    writeln!(vcd, "   SUMP3 Capture").ok();
    writeln!(vcd, "$end").ok();
    writeln!(
        vcd,
        "$version\n   SUMP3 ILA - {} / {}\n$end",
        hub_name, pod_name
    )
    .ok();
    writeln!(vcd, "$timescale {}ns $end", period_ns).ok();
    writeln!(vcd, "$scope module {} $end", scope_name).ok();

    // Signal definitions
    for (idx, sig) in signals.iter().enumerate() {
        let id = signal_id(idx);
        let width = sig.bit_high - sig.bit_low + 1;
        // Clean up signal name - extract base name without bit range
        let clean_name = clean_signal_name(&sig.name);
        
        if sig.signal_type == "analog" {
            // VCD real type for analog signals (size is always 1 for reals)
            writeln!(vcd, "$var real 1 {} {} $end", id, clean_name).ok();
        } else if width == 1 {
            // Single bit - no range needed
            writeln!(vcd, "$var wire 1 {} {} $end", id, clean_name).ok();
        } else {
            // Multi-bit - add bit range in VCD format
            writeln!(vcd, "$var wire {} {} {} [{}:0] $end", width, id, clean_name, width - 1).ok();
        }
    }

    writeln!(vcd, "$upscope $end").ok();
    writeln!(vcd, "$enddefinitions $end").ok();

    // Initial values
    if let Some(first_sample) = capture.samples.first() {
        writeln!(vcd, "$dumpvars").ok();
        for (idx, sig) in signals.iter().enumerate() {
            let id = signal_id(idx);
            let width = sig.bit_high - sig.bit_low + 1;
            let value = extract_signal(first_sample.data, sig);
            write_vcd_value(&mut vcd, &id, width, value, sig.signal_type == "analog");
        }
        writeln!(vcd, "$end").ok();
    }

    // Value changes
    if capture.samples.len() > 1 {
        let mut prev_values: Vec<u32> = signals
            .iter()
            .map(|sig| {
                capture
                    .samples
                    .first()
                    .map(|s| extract_signal(s.data, sig))
                    .unwrap_or(0)
            })
            .collect();

        let mut current_time = capture.samples.first().map(|s| s.timestamp).unwrap_or(0);
        writeln!(vcd, "#{}", current_time).ok();

        for sample in capture.samples.iter().skip(1) {
            let new_time = sample.timestamp;
            let mut changes = Vec::new();

            for (idx, sig) in signals.iter().enumerate() {
                let value = extract_signal(sample.data, sig);
                if value != prev_values[idx] {
                    let id = signal_id(idx);
                    let width = sig.bit_high - sig.bit_low + 1;
                    let mut change = String::new();
                    write_vcd_value(&mut change, &id, width, value, sig.signal_type == "analog");
                    changes.push(change);
                    prev_values[idx] = value;
                }
            }

            if !changes.is_empty() {
                if new_time != current_time {
                    writeln!(vcd, "#{}", new_time).ok();
                    current_time = new_time;
                }
                for change in changes {
                    write!(vcd, "{}", change).ok();
                }
            }
        }
    }

    vcd.into_bytes()
}

/// Clean up a signal name - remove existing bit ranges and invalid characters
fn clean_signal_name(name: &str) -> String {
    // Remove bit range like [11:0] or [31:0] from the name
    let base_name = if let Some(bracket_pos) = name.find('[') {
        &name[..bracket_pos]
    } else {
        name
    };
    
    // Replace any remaining invalid VCD characters with underscore
    base_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Generate a VCD signal identifier from an index
fn signal_id(idx: usize) -> String {
    // Use printable ASCII characters starting from '!'
    // For idx 0-93, use single character; beyond that, use two characters
    if idx < 94 {
        char::from_u32(33 + idx as u32).unwrap().to_string()
    } else {
        let first = idx / 94;
        let second = idx % 94;
        format!(
            "{}{}",
            char::from_u32(33 + first as u32).unwrap(),
            char::from_u32(33 + second as u32).unwrap()
        )
    }
}

/// Extract a signal value from the data word based on bit positions
fn extract_signal(data: u32, sig: &SignalInfo) -> u32 {
    let width = sig.bit_high - sig.bit_low + 1;
    let mask = if width >= 32 {
        u32::MAX
    } else {
        (1u32 << width) - 1
    };
    (data >> sig.bit_low) & mask
}

/// Write a VCD value change
fn write_vcd_value(out: &mut String, id: &str, width: u16, value: u32, is_analog: bool) {
    if is_analog {
        // Convert to signed value for analog signals (assuming 2's complement)
        let signed_value = if width < 32 && (value & (1 << (width - 1))) != 0 {
            // Sign extend
            let sign_mask = !((1u32 << width) - 1);
            (value | sign_mask) as i32
        } else if width >= 32 {
            value as i32
        } else {
            value as i32
        };
        writeln!(out, "r{} {}", signed_value as f64, id).ok();
    } else if width == 1 {
        writeln!(out, "{}{}", value & 1, id).ok();
    } else {
        // Format as binary with leading zeros
        let binary = format!("{:0width$b}", value, width = width as usize);
        writeln!(out, "b{} {}", binary, id).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sump::{CaptureStatus, RleSample};

    #[test]
    fn test_signal_id() {
        assert_eq!(signal_id(0), "!");
        assert_eq!(signal_id(1), "\"");
        assert_eq!(signal_id(93), "~");
    }

    #[test]
    fn test_extract_signal() {
        let sig = SignalInfo {
            name: "test".to_string(),
            bit_high: 7,
            bit_low: 0,
            signal_type: "vector".to_string(),
        };
        assert_eq!(extract_signal(0xFF, &sig), 0xFF);
        assert_eq!(extract_signal(0xABCD, &sig), 0xCD);

        let sig2 = SignalInfo {
            name: "test".to_string(),
            bit_high: 11,
            bit_low: 8,
            signal_type: "vector".to_string(),
        };
        assert_eq!(extract_signal(0xABCD, &sig2), 0xB);
    }

    #[test]
    fn test_generate_simple_vcd() {
        let capture = CaptureData {
            hub: 0,
            pod: 0,
            ts_bits: 8,
            data_bits: 8,
            status: CaptureStatus {
                armed: false,
                pre_trigger: false,
                triggered: true,
                acquired: true,
                init_in_progress: false,
            },
            samples: vec![
                RleSample {
                    address: 0,
                    code: 2,
                    timestamp: 0,
                    data: 0x00,
                },
                RleSample {
                    address: 1,
                    code: 3,
                    timestamp: 10,
                    data: 0xFF,
                },
            ],
            sample_count: 2,
        };

        let vcd = generate_vcd(&capture, None, None);
        let vcd_str = String::from_utf8(vcd).unwrap();

        assert!(vcd_str.contains("$timescale"));
        assert!(vcd_str.contains("$var wire"));
        assert!(vcd_str.contains("#0"));
        assert!(vcd_str.contains("#10"));
    }
}
