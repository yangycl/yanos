//! USB device abstraction and context structures.

use crate::{
    Dma, Result, UsbError,
    desc::{ConfigDesc, DeviceDesc, EndpointDesc, SetupPacket, desc_type},
    reg,
    ring::{PhysMem, Ring, Trb, completion, trb_type},
    xhci::XhciCtrl,
};

use alloc::{sync::Arc, vec::Vec};
use core::hint::spin_loop;
use spin::Mutex;

/// xHCI Slot Context. 64 bytes because Intel client xHCI has HCCPARAMS1.CSZ = 1.
/// A 32-byte slot makes Address Device return completion code 17.
#[repr(C, align(64))]
#[derive(Clone, Copy, Default)]
pub struct SlotContext {
    /// Route String, Speed, Multi-TT, Hub, Context Entries
    pub dw0: u32,
    /// Max Exit Latency, Root Hub Port Number, Number of Ports
    pub dw1: u32,
    /// Interrupter Target, TTT, TT Port Number, TT Hub Slot ID
    pub dw2: u32,
    /// Device Address, Slot State
    pub dw3: u32,
    _0: [u32; 12],
}

impl SlotContext {
    /// Creates a new Slot Context.
    pub fn new(route: u32, speed: u8, context_entries: u8, root_port: u8) -> Self {
        Self {
            dw0: (route & 0xfffff) | ((speed as u32) << 20) | ((context_entries as u32) << 27),
            dw1: ((root_port as u32) << 16),
            dw2: 0,
            dw3: 0,
            _0: [0; 12],
        }
    }
}

/// xHCI Endpoint Context. 64 bytes to match CSZ = 1.
#[repr(C, align(64))]
#[derive(Clone, Copy, Default)]
pub struct EndpointContext {
    /// EP State, Mult, MaxPStreams, LSA, Interval, Max ESIT Payload Hi
    pub dw0: u32,
    /// CErr, EP Type, HID, Max Burst Size, Max Packet Size
    pub dw1: u32,
    /// Transfer Ring Dequeue Pointer Low
    pub tr_dequeue_lo: u32,
    /// Transfer Ring Dequeue Pointer High
    pub tr_dequeue_hi: u32,
    /// Average TRB Length, Max ESIT Payload Lo
    pub dw4: u32,
    _0: [u32; 11],
}

impl EndpointContext {
    /// Creates a new Endpoint Context.
    pub fn new(
        ep_type: u8,
        max_packet_size: u16,
        max_burst: u8,
        interval: u8,
        tr_ptr: u64,
    ) -> Self {
        Self {
            dw0: (interval as u32) << 16,
            dw1: ((3u32) << 1)
                | ((ep_type as u32) << 3)
                | ((max_burst as u32) << 8)
                | ((max_packet_size as u32) << 16),
            tr_dequeue_lo: (tr_ptr as u32) | 1, // DCS = 1
            tr_dequeue_hi: (tr_ptr >> 32) as u32,
            dw4: 8, // Average TRB Length
            _0: [0; 11],
        }
    }
}

/// xHCI Input Context for Address Device / Configure Endpoint commands.
///
/// Used to pass configuration data to the xHCI controller when
/// addressing a device or configuring endpoints.
#[repr(C, align(64))]
#[derive(Default)]
pub struct InputContext {
    /// Input Control Context (drop/add flags)
    pub input_control: [u32; 16],
    /// Slot Context
    pub slot: SlotContext,
    /// Endpoint Contexts (EP0 at index 0, EP1 OUT at 1, EP1 IN at 2, etc.)
    pub endpoints: [EndpointContext; 31],
}

/// xHCI Device Context.
///
/// Output context maintained by the xHCI controller containing
/// the current state of a USB device's slot and endpoints.
#[repr(C, align(64))]
#[derive(Default)]
pub struct DeviceContext {
    /// Slot Context
    pub slot: SlotContext,
    /// Endpoint Contexts
    pub endpoints: [EndpointContext; 31],
}

/// USB Device abstraction.
///
/// Represents an addressed USB device connected to an xHCI controller.
/// Provides methods for control transfers, device enumeration, and
/// endpoint configuration.
pub struct UsbDevice<H: Dma> {
    ctrl: Arc<XhciCtrl<H>>,
    slot_id: u8,
    port: u8,
    speed: u8,
    ep0_mps: u16,
    last_in: [u8; 4],
    device_ctx: PhysMem<H>,
    input_ctx: PhysMem<H>,
    ep0_ring: Mutex<Ring<H>>,
    ep_rings: Mutex<Vec<Option<Ring<H>>>>,
    device_desc: Option<DeviceDesc>,
}

impl<H: Dma> UsbDevice<H> {
    /// Create and address a new USB device
    pub fn new(ctrl: Arc<XhciCtrl<H>>, port: u8) -> Result<Self> {
        let host = ctrl.host();

        // Enable slot
        let slot_id = ctrl.enable_slot()?;

        // Reset port and get speed
        ctrl.reset_port(port)?;
        let speed = ctrl.port_speed(port);
        if speed == 0 {
            return Err(UsbError::InvPort);
        }

        // Allocate contexts
        let device_ctx = PhysMem::alloc(
            host,
            core::mem::size_of::<DeviceContext>(),
            core::mem::align_of::<DeviceContext>(),
        )?;
        let input_ctx = PhysMem::alloc(
            host,
            core::mem::size_of::<InputContext>(),
            core::mem::align_of::<InputContext>(),
        )?;

        // Allocate EP0 transfer ring
        let ep0_ring = Ring::new(host, 256)?;

        // Setup Input Context
        let input = input_ctx.as_ptr::<InputContext>();
        let max_packet = match speed {
            reg::SPEED_LOW => 8,
            reg::SPEED_FULL => 8,
            reg::SPEED_HIGH => 64,
            reg::SPEED_SUPER => 512,
            _ => 8,
        };
        unsafe {
            // Add flags: Slot Context (bit 0) + EP0 Context (bit 1)
            (*input).input_control[1] = 0b11;

            // Slot Context
            (*input).slot = SlotContext::new(0, speed, 1, port + 1);

            // EP0 Context (Control endpoint)
            (*input).endpoints[0] = EndpointContext::new(
                4, // Control Bidirectional
                max_packet,
                0,
                0,
                ep0_ring.phys(host),
            );
        }

        // Set device context in DCBAA
        ctrl.set_device_context(slot_id, device_ctx.phys(host));

        // Address Device command
        let trb = Trb {
            param: input_ctx.phys(host),
            status: 0,
            control: (trb_type::ADDRESS_DEVICE << 10) | ((slot_id as u32) << 24),
        };
        ctrl.submit_command(trb)?;

        // Allocate ep_rings on heap to reduce stack usage
        let mut ep_rings = Vec::with_capacity(31);
        ep_rings.resize_with(31, || None);

        Ok(Self {
            ctrl,
            slot_id,
            port,
            speed,
            ep0_mps: max_packet,
            last_in: [0; 4],
            device_ctx,
            input_ctx,
            ep0_ring: Mutex::new(ep0_ring),
            ep_rings: Mutex::new(ep_rings),
            device_desc: None,
        })
    }

    /// Perform a control transfer
    pub fn control_transfer(
        &self,
        setup: &SetupPacket,
        mut data: Option<&mut [u8]>,
    ) -> Result<usize> {
        let host = self.ctrl.host();
        let mut ep0_ring = self.ep0_ring.lock();

        let data_dir = (setup.request_type & 0x80) != 0; // true = IN
        let data_len = data.as_ref().map(|d| d.len()).unwrap_or(0);

        // Allocate data buffer if needed
        // Use 64-byte alignment for DMA efficiency (cache line size)
        let data_buf = if data_len > 0 {
            let buf = PhysMem::alloc(host, data_len, 64)?;
            if !data_dir {
                // OUT: copy data to buffer
                if let Some(ref d) = data {
                    unsafe {
                        core::ptr::copy_nonoverlapping(d.as_ptr(), buf.as_ptr(), d.len());
                    }
                }
            }
            Some(buf)
        } else {
            None
        };

        // Setup Stage TRB
        let setup_trb = Trb {
            param: unsafe { *(setup as *const SetupPacket as *const u64) },
            status: 8, // Transfer length = 8
            control: (trb_type::SETUP << 10)
                | (1 << 6) // IDT (Immediate Data)
                | if data_len > 0 && setup.length > 0 {
                    if data_dir { 3 << 16 } else { 2 << 16 } // TRT: IN or OUT
                } else {
                    0 // No data stage
                },
        };
        ep0_ring.enqueue(host, setup_trb);

        // Data stage, one TRB per EP0 packet. A single 254-byte TRB comes back
        // as a 9-byte header on this controller, so the interface bytes stay zero.
        let mps = self.ep0_mps as usize;
        if let Some(ref buf) = data_buf {
            let mut left = (setup.length as usize).min(data_len);
            let mut off = 0;
            while left > 0 {
                let chunk = left.min(mps);
                left -= chunk;
                let last = left == 0;
                let data_trb = Trb {
                    param: buf.phys(host) + off as u64,
                    status: chunk as u32,
                    control: (trb_type::DATA << 10)
                        | if data_dir { 1 << 16 } else { 0 }
                        | (1 << 2)
                        | if last { 0 } else { 1 << 4 }
                };
                ep0_ring.enqueue(host, data_trb);
                off += chunk;
            }
        }

        // Status Stage TRB
        let status_trb = Trb {
            param: 0,
            status: 0,
            control: (trb_type::STATUS << 10)
                | if data_len > 0 && setup.length > 0 && data_dir { 0 } else { 1 << 16 } // DIR
                | (1 << 5), // IOC
        };
        ep0_ring.enqueue(host, status_trb);

        drop(ep0_ring);

        // Ring doorbell for EP0 (target = 1)
        self.ctrl.ring_doorbell(self.slot_id, 1);

        // Only the status TRB has IOC. Copy after that event; copying the
        // first event was reading a buffer the device had not written yet.
        let mut transferred = 0usize;
        let mut done = false;
        for _ in 0..5_000_000 {
            if let Some(evt) = self.ctrl.poll_event()
                && evt.trb_type() == trb_type::TRANSFER_EVENT as u8
                && evt.slot_id() == self.slot_id
            {
                let code = evt.completion_code();
                match code {
                    completion::SUCCESS | completion::SHORT_PACKET => {
                        transferred = if data_len == 0 {
                            0
                        } else {
                            (setup.length as usize)
                                .saturating_sub(evt.transfer_length() as usize)
                                .min(data_len)
                        };
                        if data_dir && let (Some(buf), Some(d)) = (&data_buf, &mut data) {
                            unsafe {
                                for i in 0..transferred {
                                    d[i] = core::ptr::read_volatile(buf.as_ptr::<u8>().add(i));
                                }
                            }
                            if transferred > 0 && d[..transferred].iter().all(|b| *b == 0) {
                                transferred = 0;
                            }
                        }
                        done = true;
                    }
                    completion::STALL_ERROR => {
                        if let Some(buf) = data_buf {
                            buf.free(host);
                        }
                        return Err(UsbError::Stall);
                    }
                    _ => {
                        if let Some(buf) = data_buf {
                            buf.free(host);
                        }
                        return Err(UsbError::XferFail(code));
                    }
                }
            } else {
                spin_loop();
            }
            if done {
                if let Some(buf) = data_buf {
                    buf.free(host);
                }
                if data_len > 0 && transferred == 0 {
                    return Err(UsbError::Timeout);
                }
                return Ok(transferred);
            }
        }
        if let Some(buf) = data_buf {
            buf.free(host);
        }
        Err(UsbError::Timeout)
    }

    /// Full-speed EP0 may be 8, 16, 32, or 64. Read the first 8 bytes of the
    /// device descriptor and evaluate the context before any larger transfer.
    pub fn sync_ep0_packet(&mut self) -> Result<u16> {
        if self.speed != reg::SPEED_FULL && self.speed != reg::SPEED_LOW {
            return Ok(self.ep0_mps);
        }
        let mut buf = [0u8; 8];
        let setup = SetupPacket::get_descriptor(desc_type::DEVICE, 0, 8);
        let n = self.control_transfer(&setup, Some(&mut buf))?;
        self.last_in = [buf[0], buf[1], buf[2], buf[7]];
        if n < 8 || buf[0] < 8 || buf[1] != 1 {
            return Ok(self.ep0_mps);
        }
        let mps = buf[7] as u16;
        if mps != 8 && mps != 16 && mps != 32 && mps != 64 {
            return Ok(self.ep0_mps);
        }
        if mps == self.ep0_mps {
            return Ok(mps);
        }
        let host = self.ctrl.host();
        let input = self.input_ctx.as_ptr::<InputContext>();
        unsafe {
            (*input).input_control[0] = 0;
            (*input).input_control[1] = 1 << 1;
            (*input).endpoints[0].dw1 = ((*input).endpoints[0].dw1 & 0x0000_ffff)
                | ((mps as u32) << 16);
        }
        let trb = Trb {
            param: self.input_ctx.phys(host),
            status: 0,
            control: (trb_type::EVALUATE_CONTEXT << 10) | ((self.slot_id as u32) << 24),
        };
        self.ctrl.submit_command(trb)?;
        self.ep0_mps = mps;
        Ok(mps)
    }

    /// Get device descriptor
    pub fn get_device_descriptor(&mut self) -> Result<DeviceDesc> {
        let mut buf = [0u8; 18];
        let setup = SetupPacket::get_descriptor(desc_type::DEVICE, 0, 18);
        self.control_transfer(&setup, Some(&mut buf))?;

        let desc = unsafe { *(buf.as_ptr() as *const DeviceDesc) };
        self.device_desc = Some(desc);
        Ok(desc)
    }

    /// Get configuration descriptor (full, with interfaces and endpoints)
    pub fn get_config_descriptor(&mut self, index: u8) -> Result<Vec<u8>> {
        // Full-speed devices often NAK the first GET_DESCRIPTOR. Retry until
        // the 9-byte header is real; a zero header is not an empty config.
        // Full-speed EP0 is 8 bytes. A 9-byte request needs two packets and
        // comes back without a config header on this controller.
        let ask = if self.speed <= reg::SPEED_FULL { 8 } else { 9 };
        let mut buf = [0u8; 9];
        let mut got = false;
        for _ in 0..4 {
            buf = [0u8; 9];
            let setup = SetupPacket::get_descriptor(desc_type::CONFIGURATION, index, ask);
            let n = self.control_transfer(&setup, Some(&mut buf[..ask as usize]))?;
            if n >= 4 && buf[0] == 9 && buf[1] == 2 {
                got = true;
                break;
            }
            for _ in 0..2_000_000 {
                spin_loop();
            }
        }
        if !got {
            self.last_in = [buf[0], buf[1], buf[2], buf[3]];
            return Err(UsbError::InvalidDescriptor);
        }

        let total_len = u16::from_le_bytes([buf[2], buf[3]]) as usize;
        if total_len < 9 || total_len > 1024 {
            return Err(UsbError::InvalidDescriptor);
        }

        let mut full_buf = alloc::vec![0u8; total_len];
        let setup = SetupPacket::get_descriptor(desc_type::CONFIGURATION, index, total_len as u16);
        let n = self.control_transfer(&setup, Some(&mut full_buf))?;
        full_buf.truncate(n.min(total_len));

        Ok(full_buf)
    }

    /// Set configuration
    pub fn set_configuration(&self, config: u8) -> Result<()> {
        let setup = SetupPacket::set_configuration(config);
        self.control_transfer(&setup, None)?;
        Ok(())
    }

    /// Configure an endpoint (after SET_CONFIGURATION)
    pub fn configure_endpoint(&self, ep: &EndpointDesc) -> Result<()> {
        let host = self.ctrl.host();

        let ep_num = ep.number();
        let is_in = ep.is_in();
        let ep_type = ep.transfer_type();

        // Endpoint Context Index: EP1 OUT = 2, EP1 IN = 3, EP2 OUT = 4, etc.
        let dci = (ep_num as usize * 2) + if is_in { 1 } else { 0 };
        let ring_idx = dci - 1; // rings array is 0-indexed for EP1+

        // Allocate transfer ring for this endpoint
        let ring = Ring::new(host, 256)?;
        let ring_phys = ring.phys(host);

        // Update input context
        let input = self.input_ctx.as_ptr::<InputContext>();
        unsafe {
            (*input).input_control[0] = 0; // Drop flags
            (*input).input_control[1] = (1 << dci) | 1; // Add flags: this EP + Slot

            // xHCI endpoint type encoding
            let xhci_ep_type = match (ep_type, is_in) {
                (0, _) => 4,     // Control (bidirectional)
                (1, false) => 1, // Isoch OUT
                (1, true) => 5,  // Isoch IN
                (2, false) => 2, // Bulk OUT
                (2, true) => 6,  // Bulk IN
                (3, false) => 3, // Interrupt OUT
                (3, true) => 7,  // Interrupt IN
                _ => 4,
            };

            // Calculate interval for xHCI (different from USB descriptor)
            let interval = if self.speed >= reg::SPEED_HIGH {
                ep.interval.saturating_sub(1)
            } else {
                // For FS/LS, convert ms to 125us frames
                // Use integer log2: find highest set bit
                let ms = ep.interval.max(1) as u32;
                let log2_ceil = if ms.is_power_of_two() {
                    ms.trailing_zeros() as u8
                } else {
                    (u32::BITS - ms.leading_zeros()) as u8
                };
                log2_ceil + 3
            };

            (*input).endpoints[ring_idx] =
                EndpointContext::new(xhci_ep_type, ep.max_packet_size, 0, interval, ring_phys);
        }

        // Store ring
        let mut ep_rings = self.ep_rings.lock();
        ep_rings[ring_idx] = Some(ring);
        drop(ep_rings);

        // Configure Endpoint command
        let trb = Trb {
            param: self.input_ctx.phys(host),
            status: 0,
            control: (trb_type::CONFIGURE_ENDPOINT << 10) | ((self.slot_id as u32) << 24),
        };
        self.ctrl.submit_command(trb)?;

        Ok(())
    }

    /// Queue a transfer on an endpoint
    pub fn queue_transfer(
        &self,
        ep_num: u8,
        is_in: bool,
        buf: &PhysMem<H>,
        len: usize,
    ) -> Result<()> {
        let dci = (ep_num as usize * 2) + if is_in { 1 } else { 0 };
        let ring_idx = dci - 1;

        let mut ep_rings = self.ep_rings.lock();
        let ring = ep_rings[ring_idx].as_mut().ok_or(UsbError::InvEndpoint)?;

        let host = self.ctrl.host();
        let trb = Trb {
            param: buf.phys(host),
            status: len as u32,
            control: (trb_type::NORMAL << 10) | (1 << 5), // IOC
        };
        ring.enqueue(host, trb);
        drop(ep_rings);

        // Ring doorbell
        self.ctrl.ring_doorbell(self.slot_id, dci as u8);

        Ok(())
    }

    /// Returns the xHCI slot ID assigned to this device.
    pub fn slot_id(&self) -> u8 {
        self.slot_id
    }

    /// Returns the root hub port number this device is connected to.
    pub fn port(&self) -> u8 {
        self.port
    }

    /// Returns the device speed (see `reg::SPEED_*` constants).
    pub fn speed(&self) -> u8 {
        self.speed
    }

    /// First bytes from the last descriptor read.
    pub fn last_in(&self) -> [u8; 4] {
        self.last_in
    }

    /// Returns a reference to the xHCI controller.
    pub fn ctrl(&self) -> &Arc<XhciCtrl<H>> {
        &self.ctrl
    }
}

impl<H: Dma> Drop for UsbDevice<H> {
    fn drop(&mut self) {
        let _ = self.ctrl.disable_slot(self.slot_id);

        let host = self.ctrl.host();

        // Free endpoint rings
        let mut ep_rings = self.ep_rings.lock();
        for ring in ep_rings.iter_mut() {
            if let Some(r) = ring.take() {
                r.free(host);
            }
        }
        drop(ep_rings);

        // Free EP0 ring
        let ep0_ring = core::mem::replace(&mut *self.ep0_ring.lock(), Ring::new(host, 1).unwrap());
        ep0_ring.free(host);
    }
}
