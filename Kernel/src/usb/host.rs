use alloc::sync::Arc;
use spin::Mutex;
use usb_oxide::{
    find_hid_interfaces, HidDevice, HidType, UsbDevice, UsbError, XhciCtrl,
};

use crate::dma::{MyDma, DMA_POOL};
use crate::drivers::keyboard;
use crate::usb::pci::find_xhci_bars;

static KEYBOARD: Mutex<Option<HidDevice<MyDma>>> = Mutex::new(None);
static RETRY: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static STATUS: Mutex<&'static str> = Mutex::new("NO CTRL");
static LOG: Mutex<alloc::vec::Vec<alloc::string::String>> = Mutex::new(alloc::vec::Vec::new());

pub fn status() -> &'static str {
    *STATUS.lock()
}

pub fn log_lines() -> alloc::vec::Vec<alloc::string::String> {
    LOG.lock().clone()
}

fn note(line: alloc::string::String) {
    let mut log = LOG.lock();
    if log.len() < 24 {
        log.push(line);
    }
}

fn hex16(v: usize) -> alloc::string::String {
    alloc::format!("{v:X}")
}

/// 開機時呼叫一次：每顆 xHCI 都試。接收器可能不在第一顆控制器。
pub fn init() -> bool {
    LOG.lock().clear();
    let mut bars = [0usize; 4];
    let n = find_xhci_bars(&mut bars);
    note(alloc::format!("XHCI N {n}"));
    if n == 0 {
        *STATUS.lock() = "NO CTRL";
        return false;
    }
    for (i, bar0) in bars.into_iter().take(n).enumerate() {
        note(alloc::format!("BAR{i} {}", hex16(bar0)));
        if init_one(bar0) {
            return true;
        }
    }
    false
}

fn init_one(bar0: usize) -> bool {
    let dma = unsafe {
        MyDma::new(DMA_POOL.0.as_ptr() as usize, DMA_POOL.0.len())
    };

    // 真機 BIOS 還握著 xHCI 時直接 HCRST，SMI 會把機器重開。QEMU 沒這段。
    bios_handoff(bar0);

    let ctrl = match XhciCtrl::new(bar0, dma) {
        Ok(c) => Arc::new(c),
        Err(_) => {
            note(alloc::string::String::from("CTRL FAIL"));
            *STATUS.lock() = "CTRL FAIL";
            return false;
        }
    };
    note(alloc::format!("PORTS {}", ctrl.max_ports()));

    let mut saw_port = false;
    let mut saw_dev = false;
    let mut saw_cfg = false;
    let mut saw_hid = false;
    for port in 0..ctrl.max_ports() {
        if !ctrl.port_connected(port) {
            continue;
        }
        saw_port = true;
        note(alloc::format!("CCS {port}"));
        // 真機 handoff 後裝置多半還在 Disabled，不 reset 就不會進 Addressed。
        if ctrl.reset_port(port).is_err() {
            note(alloc::format!("RST FAIL {port}"));
            continue;
        }
        let still = ctrl.port_connected(port);
        note(alloc::format!("AFTER {port} {still}"));
        if !still {
            continue;
        }

        let mut dev = match UsbDevice::new(ctrl.clone(), port) {
            Ok(dev) => dev,
            Err(err) => {
                note(alloc::format!("ADDR {port} {}", err_name(err)));
                continue;
            }
        };
        saw_dev = true;
        note(alloc::format!("ADDR OK {port}"));
        note(alloc::format!("SPD {}", dev.speed()));

        // 裝置描述子 / 設定
        let _ = dev.get_device_descriptor();
        let Ok(config) = dev.get_config_descriptor(0) else {
            note(alloc::format!("CFG FAIL {port}"));
            continue;
        };
        let b0 = config.first().copied().unwrap_or(0);
        let b2 = config.get(2).copied().unwrap_or(0);
        let b3 = config.get(3).copied().unwrap_or(0);
        note(alloc::format!("CFG {} H {b0:02X} {b2:02X}{b3:02X}", config.len()));
        if config.len() < 9 || b0 == 0 {
            continue;
        }
        let mut off = 0;
        let mut shown = 0;
        while off + 2 <= config.len() && shown < 8 {
            let len = config[off] as usize;
            let dtype = config[off + 1];
            if len == 0 || off + len > config.len() {
                note(alloc::format!("BAD {off}"));
                break;
            }
            if dtype == 4 && len >= 9 {
                note(alloc::format!(
                    "I {} {} {}",
                    config[off + 5], config[off + 6], config[off + 7]
                ));
                shown += 1;
            } else if dtype == 5 && len >= 7 {
                note(alloc::format!("E {:02X} {:02X}", config[off + 2], config[off + 3]));
                shown += 1;
            }
            off += len;
        }
        saw_cfg = true;
        let cfg_val = config.get(5).copied().unwrap_or(0);
        note(alloc::format!("VAL {cfg_val}"));
        // 通常 bConfigurationValue 在 config[5]
        if config.len() > 5 {
            let set_ok = dev.set_configuration(config[5]).is_ok();
            note(alloc::format!("SETCFG {set_ok}"));
        }

        let dev = Arc::new(dev);
        let hid_list = find_hid_interfaces(&config);
        note(alloc::format!("HIDN {}", hid_list.len()));

        for (iface, ep) in hid_list.iter() {
            let Ok(hid) = HidDevice::from_interface(dev.clone(), iface, ep) else {
                note(alloc::string::String::from("HID NEW FAIL"));
                continue;
            };
            saw_hid = true;
            let kind = match hid.hid_type() {
                HidType::Keyboard => "KBD",
                HidType::Mouse => "MOU",
                HidType::Other => "OTH",
            };
            note(alloc::format!("IF {kind}"));
            // 很多 2.4G 接收器是 report protocol，subclass 不是 boot，HidType 會是 Other。
            if hid.hid_type() == HidType::Mouse {
                continue;
            }
            let _ = hid.set_protocol(0);
            let _ = hid.queue_read();
            *KEYBOARD.lock() = Some(hid);
            *STATUS.lock() = "HID KBD OK";
            return true;
        }
    }
    let status = if !saw_port {
        "NO PORT"
    } else if !saw_dev {
        "ADDR FAIL"
    } else if !saw_cfg {
        "NO CFG"
    } else if !saw_hid {
        "NO HID IF"
    } else {
        "MOUSE ONLY"
    };
    // 後面的空控制器不要把前面的 ADDR FAIL 蓋成 NO PORT。
    if saw_port || *STATUS.lock() == "NO CTRL" || *STATUS.lock() == "NO PORT" {
        *STATUS.lock() = status;
    }
    false
}

fn err_name(err: UsbError) -> alloc::string::String {
    match err {
        UsbError::Timeout => alloc::string::String::from("TIMEOUT"),
        UsbError::OoRam => alloc::string::String::from("OORAM"),
        UsbError::MapFail => alloc::string::String::from("MAP"),
        UsbError::InvSlot => alloc::string::String::from("SLOT"),
        UsbError::InvPort => alloc::string::String::from("PORT"),
        UsbError::InvEndpoint => alloc::string::String::from("EP"),
        UsbError::CmdFail(code) => alloc::format!("CMD {code}"),
        UsbError::XferFail(code) => alloc::format!("XFER {code}"),
        UsbError::DeviceNotFound => alloc::string::String::from("NODEV"),
        UsbError::NotSupported => alloc::string::String::from("NOSUP"),
        UsbError::InvalidDescriptor => alloc::string::String::from("DESC"),
        UsbError::Stall => alloc::string::String::from("STALL"),
    }
}

/// 主迴圈每圈呼叫：有鍵就推進 keyboard 佇列
pub fn poll() {
    if KEYBOARD.lock().is_none() {
        let n = RETRY.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        // 接收器開機後才插上也要看得到。不要每一圈都 HCRST。
        if n % 2000 == 0 {
            let _ = init();
        }
        return;
    }
    let guard = KEYBOARD.lock();
    let Some(hid) = guard.as_ref() else {
        return;
    };
    if let Some(report) = hid.poll_keyboard() {
        keyboard::push_keyboard_report(&report);
    }
}
/// xHCI extended capability 1：跟 BIOS 要 controller。
/// 沒做這步就寫 USBCMD，實機上的 legacy SMI 常常直接 reset。
fn bios_handoff(bar0: usize) {
    let Some(mmio) = (unsafe { crate::memory::paging::map_mmio(bar0 as u64, 0x1000) }) else {
        return;
    };
    let base = mmio as *mut u32;
    let hccparams1 = unsafe { core::ptr::read_volatile(base.add(0x10 / 4)) };
    let mut off = ((hccparams1 >> 16) & 0xFFFF) as usize * 4;
    if off == 0 {
        return;
    }
    for _ in 0..16 {
        if off >= 0x1000 - 8 {
            return;
        }
        let cap = unsafe { core::ptr::read_volatile(base.add(off / 4)) };
        let id = cap & 0xFF;
        let next = ((cap >> 8) & 0xFF) as usize;
        if id == 1 {
            let leg = base.wrapping_add(off / 4);
            let smi = base.wrapping_add(off / 4 + 1);
            unsafe {
                // 先關掉 ownership-change SMI，不然設 OS owned 當下就重開。
                core::ptr::write_volatile(smi, 0);
                let mut sem = core::ptr::read_volatile(leg);
                sem |= 1 << 24;
                core::ptr::write_volatile(leg, sem);
            }
            for _ in 0..1_000_000 {
                let sem = unsafe { core::ptr::read_volatile(leg) };
                if sem & (1 << 16) == 0 {
                    break;
                }
                core::hint::spin_loop();
            }
            unsafe { core::ptr::write_volatile(smi, 0) };
            return;
        }
        if next == 0 {
            return;
        }
        off += next * 4;
    }
}
