use alloc::sync::Arc;
use spin::Mutex;
use usb_oxide::{
    find_hid_interfaces, HidDevice, HidType, UsbDevice, XhciCtrl,
};

use crate::drivers::dma::{MyDma, DMA_POOL};
use crate::drivers::keyboard;
use crate::drivers::pci::find_xhci_bars;

static KEYBOARD: Mutex<Option<HidDevice<MyDma>>> = Mutex::new(None);
static RETRY: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// 開機時呼叫一次：每顆 xHCI 都試。接收器可能不在第一顆控制器。
pub fn init() -> bool {
    let mut bars = [0usize; 4];
    let n = find_xhci_bars(&mut bars);
    for bar0 in bars.into_iter().take(n) {
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
        Err(_) => return false,
    };

    for port in 0..ctrl.max_ports() {
        if !ctrl.port_connected(port) {
            continue;
        }
        // 真機 handoff 後裝置多半還在 Disabled，不 reset 就不會進 Addressed。
        let _ = ctrl.reset_port(port);
        if !ctrl.port_connected(port) {
            continue;
        }

        let Ok(mut dev) = UsbDevice::new(ctrl.clone(), port) else {
            continue;
        };

        // 裝置描述子 / 設定
        let _ = dev.get_device_descriptor();
        let Ok(config) = dev.get_config_descriptor(0) else {
            continue;
        };
        // 通常 bConfigurationValue 在 config[5]
        if config.len() > 5 {
            let _ = dev.set_configuration(config[5]);
        }

        let dev = Arc::new(dev);
        let hid_list = find_hid_interfaces(&config);

        for (iface, ep) in hid_list.iter() {
            let Ok(hid) = HidDevice::from_interface(dev.clone(), iface, ep) else {
                continue;
            };
            if hid.hid_type() == HidType::Keyboard {
                let _ = hid.set_protocol(0); // Boot Protocol
                let _ = hid.queue_read();
                *KEYBOARD.lock() = Some(hid);
                return true;
            }
        }
    }

    false
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
