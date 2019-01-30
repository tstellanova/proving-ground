#![deny(warnings)]
#![no_std]
#![no_main]

#[allow(unused)]
use panic_abort;

use core::fmt::{self, Write};

use hal::prelude::*;
use hal::serial;
use hal::time::Bps;
use nb;
use cortex_m_rt::{entry, exception, ExceptionFrame};

#[entry]
fn main() -> ! {
    let device = hal::stm32f30x::Peripherals::take().unwrap();
    let mut rcc = device.RCC.constrain();
    let mut flash = device.FLASH.constrain();
    let clocks = rcc
        .cfgr
        .sysclk(64.mhz())
        .pclk1(32.mhz())
        .pclk2(36.mhz())
        .freeze(&mut flash.acr);
    let gpioa = device.GPIOA.split(&mut rcc.ahb);
    let mut serial = device
        .USART2
        .serial((gpioa.pa2, gpioa.pa15), Bps(115200), clocks);
    serial.listen(serial::Event::Rxne);
    let (mut tx, mut rx) = serial.split();
    // COBS frame
    tx.write(0x00).unwrap();
    let mut l = Logger { tx };
    write!(l, "starting loop...\r\n").unwrap();
    loop {
        match rx.read() {
            Ok(b) => {
                l.tx.write(b).unwrap();
            }
            Err(nb::Error::Other(e)) => match e {
                serial::Error::Overrun => {
                    rx.clear_overrun_error();
                }
                _ => {
                    write!(l, "read error: {:?}", e).unwrap();
                }
            },
            Err(nb::Error::WouldBlock) => {}
        };
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

#[exception]
fn HardFault(ef: &ExceptionFrame) -> ! {
    panic!("HardFault at {:#?}", ef);
}

#[exception]
fn DefaultHandler(irqn: i16) {
    panic!("Unhandled exception (IRQn = {})", irqn);
}
