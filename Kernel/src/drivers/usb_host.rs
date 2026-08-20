use alloc::sync::Arc;
use spin::Mutex;
use usb_oxide::{
    find_hid_interfaces, HidDevice, HidType, UsbDevice, XhciCtrl,
};

use crate::drivers::dma::{MyDma, DMA_POOL};
use crate::drivers::keyboard;
use crate::drivers::pci::find_xhci_bar0;

static KEYBOARD: Mutex<Option<HidDevice<MyDma>>> = Mutex::new(None);

/// 開機時呼叫一次：找 xHCI → 初始化 → 找鍵盤
pub fn init() -> bool {
    let Some(bar0) = find_xhci_bar0() else {
        return false;
    };

    let dma = unsafe {
        MyDma::new(DMA_POOL.0.as_ptr() as usize, DMA_POOL.0.len())
    };

    let ctrl = match XhciCtrl::new(bar0, dma) {
        Ok(c) => Arc::new(c),
        Err(_) => return false,
    };

    for port in 0..ctrl.max_ports() {
        if !ctrl.port_connected(port) {
            // 可選：let _ = ctrl.reset_port(port);
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
    let guard = KEYBOARD.lock();
    let Some(hid) = guard.as_ref() else {
        return;
    };
    if let Some(report) = hid.poll_keyboard() {
        keyboard::push_keyboard_report(&report);
    }
}