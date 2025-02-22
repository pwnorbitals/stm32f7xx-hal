//! USB OTG high-speed peripheral
//!
//! Requires the `usb_hs` feature.
//! Only one of the `usb_fs`/`usb_hs` features can be selected at the same time.

use crate::pac;

use crate::gpio::{
    gpiob::{PB14, PB15},
    Alternate,
};
use crate::rcc::{Clocks, Enable, Reset};
use embedded_time::rate::Hertz;

#[cfg(feature = "usb_hs_phy")]
use synopsys_usb_otg::PhyType;
pub use synopsys_usb_otg::UsbBus;
use synopsys_usb_otg::UsbPeripheral;

pub struct USB {
    pub usb_global: pac::OTG_HS_GLOBAL,
    pub usb_device: pac::OTG_HS_DEVICE,
    pub usb_pwrclk: pac::OTG_HS_PWRCLK,
    #[cfg(feature = "usb_hs_phy")]
    pub usb_phy: Option<pac::USBPHYC>,
    pub pin_dm: PB14<Alternate<12>>,
    pub pin_dp: PB15<Alternate<12>>,
    pub hclk: Hertz,
    #[cfg(feature = "usb_hs_phy")]
    pub hse: Hertz,
}

impl USB {
    /// Construct a USB peripheral wrapper.
    ///
    /// Call `UsbBus::new` to construct and initialize the USB peripheral driver.
    pub fn new(
        usb_global: pac::OTG_HS_GLOBAL,
        usb_device: pac::OTG_HS_DEVICE,
        usb_pwrclk: pac::OTG_HS_PWRCLK,
        pins: (PB14<Alternate<12>>, PB15<Alternate<12>>),
        clocks: Clocks,
    ) -> Self {
        Self {
            usb_global,
            usb_device,
            usb_pwrclk,
            #[cfg(feature = "usb_hs_phy")]
            usb_phy: None,
            pin_dm: pins.0,
            pin_dp: pins.1,
            hclk: clocks.hclk(),
            #[cfg(feature = "usb_hs_phy")]
            hse: clocks.hse().unwrap_or_else(|| Hertz(0)),
        }
    }

    #[cfg(feature = "usb_hs_phy")]
    /// Construct a USB peripheral wrapper with internal HighSpeed PHY.
    ///
    /// Call `UsbBus::new` to construct and initialize the USB peripheral driver.
    pub fn new_with_internal_hs_phy(
        usb_global: pac::OTG_HS_GLOBAL,
        usb_device: pac::OTG_HS_DEVICE,
        usb_pwrclk: pac::OTG_HS_PWRCLK,
        usb_phy: pac::USBPHYC,
        pins: (PB14<Alternate<12>>, PB15<Alternate<12>>),
        clocks: Clocks,
    ) -> Self {
        Self {
            usb_global,
            usb_device,
            usb_pwrclk,
            usb_phy: Some(usb_phy),
            pin_dm: pins.0,
            pin_dp: pins.1,
            hclk: clocks.hclk(),
            hse: clocks.hse().expect("HSE should be enabled"),
        }
    }
}

unsafe impl Sync for USB {}

unsafe impl UsbPeripheral for USB {
    const REGISTERS: *const () = pac::OTG_HS_GLOBAL::ptr() as *const ();

    const HIGH_SPEED: bool = true;
    const FIFO_DEPTH_WORDS: usize = 1024;
    const ENDPOINT_COUNT: usize = 9;

    fn enable() {
        cortex_m::interrupt::free(|_| unsafe {
            // Enable USB peripheral
            pac::OTG_HS_GLOBAL::enable_unchecked();

            // Reset USB peripheral
            pac::OTG_HS_GLOBAL::reset_unchecked();

            #[cfg(feature = "usb_hs_phy")]
            {
                // Enable and reset HS PHY
                let rcc = &*pac::RCC::ptr();
                rcc.ahb1enr.modify(|_, w| w.otghsulpien().enabled());
                pac::USBPHYC::enable_unchecked();
                pac::USBPHYC::reset_unchecked();
            }
        });
    }

    fn ahb_frequency_hz(&self) -> u32 {
        self.hclk.0
    }

    #[cfg(feature = "usb_hs_phy")]
    #[inline(always)]
    fn phy_type(&self) -> PhyType {
        if self.usb_phy.is_some() {
            PhyType::InternalHighSpeed
        } else {
            PhyType::InternalFullSpeed
        }
    }

    #[cfg(feature = "usb_hs_phy")]
    // Setup LDO and PLL
    fn setup_internal_hs_phy(&self) {
        let phy = if let Some(phy) = self.usb_phy.as_ref() {
            phy
        } else {
            // This should never happen as this function is only called when
            // phy_type() is PhyType::InternalHighSpeed and it's possible only
            // when self.usb_phy is not None
            unreachable!()
        };

        // Calculate PLL1SEL
        let pll1sel = match self.hse.0 {
            12_000_000 => 0b000,
            12_500_000 => 0b001,
            16_000_000 => 0b011,
            24_000_000 => 0b100,
            25_000_000 => 0b101,
            _ => panic!("HSE frequency is invalid for USBPHYC"),
        };

        // Turn on LDO
        // For some reason setting the bit enables the LDO
        phy.ldo.modify(|_, w| w.ldo_disable().set_bit());

        // Busy wait until ldo_status becomes true
        // Notice, this may hang
        while phy.ldo.read().ldo_status().bit_is_clear() {}

        // Setup PLL
        // This disables the the pll1 during tuning
        phy.pll1.write(|w| unsafe { w.pll1sel().bits(pll1sel) });

        phy.tune.modify(|r, w| unsafe { w.bits(r.bits() | 0xF13) });

        phy.pll1.modify(|_, w| w.pll1en().set_bit());

        // 2ms Delay required to get internal phy clock stable
        cortex_m::asm::delay(432000);
    }
}

pub type UsbBusType = UsbBus<USB>;
