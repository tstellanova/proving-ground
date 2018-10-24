#![deny(warnings)]
#![no_std]
#![no_main]
#![feature(core_intrinsics)]

use core::fmt::{self, Write};
use core::intrinsics;
use core::panic::PanicInfo;

use cortex_m_rt::{entry, exception, ExceptionFrame};
use hal::prelude::*;
use hal::serial;
use hal::stm32f30x::interrupt;
use hal::time::Bps;
use nb;

use lsm303c::Lsm303c;

static mut L: Option<Logger<hal::serial::Tx<hal::stm32f30x::USART1>>> = None;
static mut RX: Option<hal::serial::Rx<hal::stm32f30x::USART1>> = None;
static mut QUIET: bool = true;
const TURN_QUIET: u8 = 'q' as u8;

#[entry]
fn main() -> ! {
    let device = hal::stm32f30x::Peripherals::take().unwrap();
    let core = cortex_m::Peripherals::take().unwrap();
    let mut rcc = device.RCC.constrain();
    let mut flash = device.FLASH.constrain();
    let clocks = rcc
        .cfgr
        .sysclk(64.mhz())
        .pclk1(32.mhz())
        .pclk2(32.mhz())
        .freeze(&mut flash.acr);
    let gpioa = device.GPIOA.split(&mut rcc.ahb);
    let gpiob = device.GPIOB.split(&mut rcc.ahb);
    let mut serial = device
        .USART1
        .serial((gpioa.pa9, gpioa.pa10), Bps(115200), clocks);
    let ser_int = serial.get_interrupt();
    serial.listen(serial::Event::Rxne);
    let (mut tx, rx) = serial.split();
    // COBS frame
    tx.write(0x00).unwrap();
    unsafe {
        L = Some(Logger { tx });
        RX = Some(rx);
    };
    let l = unsafe { extract(&mut L) };
    write!(l, "logger ok\r\n");
    // I2C
    let i2c = device.I2C1.i2c((gpiob.pb6, gpiob.pb7), 400.khz(), clocks);
    write!(l, "i2c ok\r\n");
    // lsm
    let mut lsm303 = Lsm303c::default(i2c).expect("lsm error");
    write!(l, "lsm ok\r\n");
    // done
    unsafe { cortex_m::interrupt::enable() };
    let mut nvic = core.NVIC;
    nvic.enable(ser_int);
    write!(l, "All ok; Press 'q' to toggle verbosity!\r\n");
    loop {
        match lsm303.mag() {
            Ok(meas) => {
                if unsafe { !QUIET } {
                    write!(l, "lsm: mag({},{},{})\r\n", meas.x, meas.y, meas.z,);
                }
            }
            Err(e) => {
                write!(l, "Err meas: {:?}", e);
            }
        };
        match lsm303.unscaled_mag() {
            Ok(meas) => {
                if unsafe { !QUIET } {
                    write!(l, "lsm: unscmag({},{},{})\r\n", meas.x, meas.y, meas.z,);
                }
            }
            Err(e) => {
                write!(l, "Err meas: {:?}", e);
            }
        }
    }
}

unsafe fn extract<T>(opt: &'static mut Option<T>) -> &'static mut T {
    match opt {
        Some(ref mut x) => &mut *x,
        None => panic!("extract"),
    }
}

struct Logger<W: ehal::serial::Write<u8>> {
    tx: W,
}
impl<W: ehal::serial::Write<u8>> fmt::Write for Logger<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            match self.write_char(c) {
                Ok(_) => {}
                Err(_) => {}
            }
        }
        match self.tx.flush() {
            Ok(_) => {}
            Err(_) => {}
        };

        Ok(())
    }

    fn write_char(&mut self, s: char) -> fmt::Result {
        match nb::block!(self.tx.write(s as u8)) {
            Ok(_) => {}
            Err(_) => {}
        }
        Ok(())
    }
}

interrupt!(USART1_EXTI25, usart_exti25);
fn usart_exti25() {
    let rx = unsafe { extract(&mut RX) };
    let l = unsafe { extract(&mut L) };
    match rx.read() {
        Ok(b) => {
            if b == TURN_QUIET {
                unsafe {
                    QUIET = !QUIET;
                }
            } else {
                // echo byte as is
                write!(l, "{}", b as char);
            }
        }
        Err(nb::Error::WouldBlock) => {}
        Err(nb::Error::Other(e)) => match e {
            serial::Error::Overrun => {
                rx.clear_overrun_error();
            }
            serial::Error::Framing => {
                rx.clear_framing_error();
            }
            serial::Error::Noise => {
                rx.clear_noise_error();
            }
            _ => {
                write!(l, "read error: {:?}", e);
            }
        },
    };
}

#[exception]
fn HardFault(ef: &ExceptionFrame) -> ! {
    panic!("HardFault at {:#?}", ef);
}

#[exception]
fn DefaultHandler(irqn: i16) {
    panic!("Unhandled exception (IRQn = {})", irqn);
}

#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    match unsafe { &mut L } {
        Some(ref mut l) => {
            let payload = panic_info.payload().downcast_ref::<&str>();
            match (panic_info.location(), payload) {
                (Some(location), Some(msg)) => {
                    write!(
                        l,
                        "\r\npanic in file '{}' at line {}: {:?}\r\n",
                        location.file(),
                        location.line(),
                        msg
                    );
                }
                (Some(location), None) => {
                    write!(
                        l,
                        "panic in file '{}' at line {}",
                        location.file(),
                        location.line()
                    );
                }
                (None, Some(msg)) => {
                    write!(l, "panic: {:?}", msg);
                }
                (None, None) => {
                    write!(l, "panic occured, no info available");
                }
            }
        }
        None => {}
    }
    unsafe { intrinsics::abort() }
}