use alloc::sync::Arc;
use spin::Mutex;
use usb_oxide::{
    find_hid_interfaces, HidDevice, HidType, MscDevice, Trb, UsbDevice, UsbError, XhciCtrl,
    trb_type,
};

use crate::dma::MyDma;
use crate::drivers::keyboard;
use crate::usb::pci::find_xhci_bars;

static KEYBOARD: Mutex<Option<HidDevice<MyDma>>> = Mutex::new(None);
static MOUSE: Mutex<Option<HidDevice<MyDma>>> = Mutex::new(None);
static MOUSE_DX: Mutex<i32> = Mutex::new(0);
static MOUSE_DY: Mutex<i32> = Mutex::new(0);
static MOUSE_BTN: Mutex<u8> = Mutex::new(0);
static STICK: Mutex<Option<MscDevice<MyDma>>> = Mutex::new(None);
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
    crate::dma::reset_cursor();
    let mut any = false;
    for (i, bar0) in bars.into_iter().take(n).enumerate() {
        note(alloc::format!("BAR{i} {}", hex16(bar0)));
        let Some(dma) = crate::dma::take_slice(256 * 1024) else {
            note(alloc::string::String::from("DMA FULL"));
            break;
        };
        // 找到鍵盤也不能停。隨身碟常常在下一顆 xHCI。
        if init_one(bar0, dma) {
            any = true;
        }
    }
    any
}

fn init_one(bar0: usize, dma: crate::dma::MyDma) -> bool {
    // 真機 BIOS 還握著 xHCI 時直接 HCRST，SMI 會把機器重開。QEMU 沒這段。
    bios_handoff(bar0);

    let ctrl = match XhciCtrl::new(bar0, dma) {
        Ok(c) => Arc::new(c),
        Err(_) => {
            note(alloc::string::String::from("CTRL FAIL"));
            if *STATUS.lock() != "HID KBD OK" {
                *STATUS.lock() = "CTRL FAIL";
            }
            return false;
        }
    };
    note(alloc::format!("PORTS {}", ctrl.max_ports()));
    note(alloc::format!("CTX {}", ctrl.context_size()));
    // Reset 後 USB2 裝置要一段時間才把 CCS 拉起來。
    for _ in 0..20 {
        if (0..ctrl.max_ports()).any(|port| ctrl.port_connected(port)) {
            break;
        }
        for _ in 0..1_000_000 {
            core::hint::spin_loop();
        }
    }

    let mut saw_port = false;
    let mut saw_dev = false;
    let mut saw_cfg = false;
    let mut saw_hid = false;
    let mut visited = 0u64;
    for pass in 0..2 {
        if pass == 1 {
            if STICK.lock().is_some() {
                break;
            }
            // USB3 隨身碟常常比接收器晚拉起 CCS。第一輪看過的 port 不再 reset。
            for _ in 0..30 {
                for _ in 0..1_000_000 {
                    core::hint::spin_loop();
                }
            }
        }
        for port in 0..ctrl.max_ports() {
        if port < 64 && visited & (1 << port) != 0 {
            continue;
        }
        if !ctrl.port_connected(port) {
            continue;
        }
        if port < 64 {
            visited |= 1 << port;
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
        match dev.sync_ep0_packet() {
            Ok(mps) => note(alloc::format!("MPS {mps}")),
            Err(err) => note(alloc::format!("MPS {}", err_name(err))),
        }

        // 裝置描述子 / 設定
        let _ = dev.get_device_descriptor();
        let config = match dev.get_config_descriptor(0) {
            Ok(config) => config,
            Err(err) => {
                let b = dev.last_in();
                note(alloc::format!(
                    "CFG FAIL {port} {} {:02X}{:02X}",
                    err_name(err), b[0], b[1]
                ));
                continue;
            }
        };
        let b0 = config.first().copied().unwrap_or(0);
        let b2 = config.get(2).copied().unwrap_or(0);
        let b3 = config.get(3).copied().unwrap_or(0);
        note(alloc::format!("CFG {} H {b0:02X} {b2:02X}{b3:02X}", config.len()));
        if config.len() < 9 || b0 == 0 {
            continue;
        }
        let mut classes = alloc::string::String::new();
        let mut off = 0;
        let mut ncls = 0;
        while off + 9 <= config.len() && ncls < 6 {
            if config[off] == 9 && config[off + 1] == 4 {
                classes.push_str(&alloc::format!(" {}", config[off + 5]));
                ncls += 1;
            }
            off += 1;
        }
        note(alloc::format!("CLS{classes}"));
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
                let _ = hid.set_protocol(0);
                let _ = hid.queue_read();
                *MOUSE.lock() = Some(hid);
                note(alloc::string::String::from("MOU OK"));
                continue;
            }
            let _ = hid.set_protocol(0);
            let _ = hid.set_idle(0, 0);
            let _ = hid.queue_read();
            *KEYBOARD.lock() = Some(hid);
            *STATUS.lock() = "HID KBD OK";
            break;
        }
        if STICK.lock().is_none() {
            if let Some(msc) = attach_msc(&dev, &config) {
                *STICK.lock() = Some(msc);
                note(alloc::string::String::from("MSC OK"));
            }
        }
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
    if *STATUS.lock() == "HID KBD OK" || MOUSE.lock().is_some() || STICK.lock().is_some() {
        return true;
    }
    if saw_port || *STATUS.lock() == "NO CTRL" || *STATUS.lock() == "NO PORT" {
        *STATUS.lock() = status;
    }
    false
}

pub fn take_stick() -> Option<MscDevice<MyDma>> {
    STICK.lock().take()
}

fn attach_msc(dev: &Arc<UsbDevice<MyDma>>, config: &[u8]) -> Option<MscDevice<MyDma>> {
    use usb_oxide::{EndpointDesc, InterfaceDesc, class, desc_type, ep_type};
    let mut off = 0;
    let mut iface: Option<InterfaceDesc> = None;
    let mut ep_in: Option<EndpointDesc> = None;
    let mut ep_out: Option<EndpointDesc> = None;
    while off + 2 <= config.len() {
        let len = config[off] as usize;
        if len < 2 || off + len > config.len() {
            off += 1;
            continue;
        }
        let dtype = config[off + 1];
        if dtype == desc_type::INTERFACE && len >= 9 && config[off + 5] == class::MASS_STORAGE {
            iface = Some(InterfaceDesc {
                length: config[off],
                desc_type: config[off + 1],
                interface_number: config[off + 2],
                alternate_setting: config[off + 3],
                num_endpoints: config[off + 4],
                interface_class: config[off + 5],
                interface_subclass: config[off + 6],
                interface_protocol: config[off + 7],
                interface: config[off + 8],
            });
            ep_in = None;
            ep_out = None;
            note(alloc::format!(
                "MSC sub {} proto {:02X}",
                config[off + 6],
                config[off + 7]
            ));
        } else if dtype == desc_type::ENDPOINT && len >= 7 && iface.is_some() {
            let addr = config[off + 2];
            let attr = config[off + 3];
            if attr & 0x03 == ep_type::BULK {
                let ep = EndpointDesc {
                    length: config[off],
                    desc_type: config[off + 1],
                    endpoint_address: addr,
                    attributes: attr,
                    max_packet_size: u16::from_le_bytes([config[off + 4], config[off + 5]]),
                    interval: config[off + 6],
                };
                if addr & 0x80 != 0 {
                    ep_in = Some(ep);
                } else {
                    ep_out = Some(ep);
                }
            }
        }
        off += len;
    }
    let Some(iface) = iface else {
        return None;
    };
    let (Some(ep_in), Some(ep_out)) = (ep_in, ep_out) else {
        note(alloc::string::String::from("MSC NOEP"));
        return None;
    };
    match MscDevice::from_interface(dev.clone(), &iface, &ep_in, &ep_out) {
        Ok(msc) => Some(msc),
        Err(err) => {
            note(alloc::format!("MSC {}", err_name(err)));
            None
        }
    }
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
pub fn take_mouse() -> (i32, i32, u8) {
    let dx = core::mem::replace(&mut *MOUSE_DX.lock(), 0);
    let dy = core::mem::replace(&mut *MOUSE_DY.lock(), 0);
    let buttons = *MOUSE_BTN.lock();
    (dx, dy, buttons)
}

pub fn poll() {
    if KEYBOARD.lock().is_none() && MOUSE.lock().is_none() {
        let n = RETRY.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        // 接收器開機後才插上也要看得到。不要每一圈都 HCRST。
        if n % 2000 == 0 {
            let _ = init();
        }
        return;
    }
    // 鍵盤和滑鼠共用同一條 event ring。先出環再依 slot+DCI 分，
    // 不能讓 poll_keyboard 把滑鼠的完成事件吃掉。
    let mut evts = [Trb::default(); 8];
    let mut n = 0;
    if let Some(hid) = KEYBOARD.lock().as_ref() {
        while n < evts.len() {
            let Some(evt) = hid.poll_event() else { break };
            evts[n] = evt;
            n += 1;
        }
    }
    if let Some(hid) = MOUSE.lock().as_ref() {
        while n < evts.len() {
            let Some(evt) = hid.poll_event() else { break };
            evts[n] = evt;
            n += 1;
        }
    }
    for evt in evts.iter().take(n) {
        if evt.trb_type() != trb_type::TRANSFER_EVENT as u8 {
            continue;
        }
        let code = evt.completion_code();
        if code != 1 && code != 13 {
            continue;
        }
        let slot = evt.slot_id();
        let ep = evt.endpoint_id();
        if let Some(hid) = KEYBOARD.lock().as_ref() {
            if hid.slot_id() == slot && hid.in_dci() == ep {
                keyboard::push_keyboard_report(&hid.take_keyboard());
                continue;
            }
        }
        if let Some(hid) = MOUSE.lock().as_ref() {
            if hid.slot_id() == slot && hid.in_dci() == ep {
                let report = hid.take_mouse();
                *MOUSE_DX.lock() += report.x as i32;
                *MOUSE_DY.lock() += report.y as i32;
                *MOUSE_BTN.lock() = report.buttons;
            }
        }
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
