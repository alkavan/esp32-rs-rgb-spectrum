#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::Level,
    rmt::{TxChannelConfig},
    timer::systimer::SystemTimer,
    time::Rate,
};
use esp_hal::rmt::{TxChannel, TxChannelCreator};
use log::info;

extern crate alloc;

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(size: 72 * 1024);
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let rmt_driver = esp_hal::rmt::Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();
    let rmt_tx = rmt_driver
        .channel0
        .configure(
            peripherals.GPIO2,
            TxChannelConfig::default()
                .with_clk_divider(1) // 80 MHz channel clock
                .with_idle_output(true)
                .with_idle_output_level(Level::Low)
                .with_carrier_modulation(false),
        )
        .unwrap();

    spawner.spawn(rgb_task(rmt_tx)).unwrap();

    loop {
        info!("Main loop running!");
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn rgb_task(mut channel: esp_hal::rmt::Channel<esp_hal::Blocking, 0>) {
    let mut hue: u16 = 0; // Hue ranges from 0 to 360

    loop {
        // Convert hue to RGB
        let (r, g, b) = hsv_to_rgb(hue, 255, 255); // Full saturation and value
        let color = [r, g, b];
        let pulses = color_to_pulses(&color);
        info!("Sending hue {} - R:{}, G:{}, B:{}", hue, r, g, b);

        let tx_transaction = channel.transmit(&pulses).unwrap();
        channel = tx_transaction.wait().unwrap();
        // info!("Transmission complete");

        // Reset period
        Timer::after(Duration::from_micros(280)).await;

        // Increment hue for smooth transition (step size affects speed)
        hue = (hue + 1) % 360;

        // Smaller delay for smooth cycling (adjust as needed)
        Timer::after(Duration::from_millis(10)).await;
    }
}

fn color_to_pulses(color: &[u8; 3]) -> [u32; 25] {
    let mut pulses = [0u32; 25];
    let color_bits = ((color[0] as u32) << 16) | ((color[1] as u32) << 8) | (color[2] as u32); // GRB order

    for i in 0..24 {
        let bit = (color_bits >> (23 - i)) & 1;
        let (high_duration, low_duration) = if bit == 1 {
            (64, 36) // Bit 1: 0.8µs high (64 ticks), 0.45µs low (36 ticks)
        } else {
            (32, 68) // Bit 0: 0.4µs high (32 ticks), 0.85µs low (68 ticks)
        };
        pulses[i] = ((high_duration as u32) & 0x7FFF) | (1 << 15) // High pulse
            | (((low_duration as u32) & 0x7FFF) << 16);           // Low pulse
    }
    pulses[24] = 0; // End marker
    pulses
}

// Simple HSV to RGB conversion (Hue: 0-360, Saturation: 0-255, Value: 0-255)
fn hsv_to_rgb(h: u16, s: u8, v: u8) -> (u8, u8, u8) {
    let h = h as u32 % 360; // Ensure hue stays within 0-359
    let s = s as u32;
    let v = v as u32;

    let region = h / 60;
    let remainder = (h - (region * 60)) * 6;

    let p = (v * (255 - s)) / 255;
    let q = (v * (255 - ((s * remainder) / 360))) / 255;
    let t = (v * (255 - ((s * (360 - remainder)) / 360))) / 255;

    match region {
        0 => (v as u8, t as u8, p as u8),
        1 => (q as u8, v as u8, p as u8),
        2 => (p as u8, v as u8, t as u8),
        3 => (p as u8, q as u8, v as u8),
        4 => (t as u8, p as u8, v as u8),
        5 => (v as u8, p as u8, q as u8),
        _ => (0, 0, 0), // Shouldn’t happen
    }
}
