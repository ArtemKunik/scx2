//! Period finishes for the voices in the amp.
//!
//! Each voice is a model of a real amplifier, so the cabinet follows the
//! original: tweed, brownface, blackface, silverface, British or modern. The
//! controls and the grille change with it, while the layout stays the same.

use eframe::egui::Color32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum Era {
    Tweed,
    Brownface,
    Blackface,
    Silverface,
    British,
    Modern,
    Metal,
}

pub const ERAS: [Era; 7] = [
    Era::Tweed,
    Era::Brownface,
    Era::Blackface,
    Era::Silverface,
    Era::British,
    Era::Modern,
    Era::Metal,
];

impl Era {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Era::Tweed => "Tweed",
            Era::Brownface => "Brownface",
            Era::Blackface => "Blackface",
            Era::Silverface => "Silverface",
            Era::British => "British",
            Era::Modern => "Modern",
            Era::Metal => "Metal",
        }
    }

    pub fn years(self) -> &'static str {
        match self {
            Era::Tweed => "1948–1960",
            Era::Brownface => "1960–1963",
            Era::Blackface => "1964–1967",
            Era::Silverface => "1968–1981",
            Era::British => "1962–1990",
            Era::Modern => "1991–today",
            Era::Metal => "2000–today",
        }
    }

    pub fn look(self) -> Look {
        match self {
            Era::Tweed => Look {
                era: self,
                weave: Weave::Tweed,
                tolex: [0.62, 0.47, 0.23],
                cloth: Cloth::Oxblood,
                panel: Panel::Brown,
                ink: Color32::from_rgb(238, 226, 198),
                knob: KnobFinish::Cream,
                pointer: Color32::from_rgb(56, 34, 18),
                pointer_shadow: Color32::from_rgb(246, 238, 214),
                jewel: JewelTint::Red,
                piping: Color32::from_rgb(198, 168, 112),
                accent: Color32::from_rgb(196, 156, 86),
            },
            Era::Brownface => Look {
                era: self,
                weave: Weave::Vinyl,
                tolex: [0.36, 0.24, 0.15],
                cloth: Cloth::Wheat,
                panel: Panel::Brown,
                ink: Color32::from_rgb(242, 228, 196),
                knob: KnobFinish::Cream,
                pointer: Color32::from_rgb(62, 38, 20),
                pointer_shadow: Color32::from_rgb(246, 236, 210),
                jewel: JewelTint::Amber,
                piping: Color32::from_rgb(206, 178, 116),
                accent: Color32::from_rgb(192, 150, 84),
            },
            Era::Blackface => Look {
                era: self,
                weave: Weave::Vinyl,
                tolex: [0.072, 0.070, 0.068],
                cloth: Cloth::Silver,
                panel: Panel::Black,
                ink: Color32::from_rgb(234, 232, 226),
                knob: KnobFinish::BlackChrome,
                pointer: Color32::from_rgb(246, 244, 238),
                pointer_shadow: Color32::from_rgb(36, 35, 33),
                jewel: JewelTint::Red,
                piping: Color32::from_rgb(196, 198, 200),
                accent: Color32::from_rgb(126, 152, 186),
            },
            Era::Silverface => Look {
                era: self,
                weave: Weave::Vinyl,
                tolex: [0.062, 0.061, 0.060],
                cloth: Cloth::Silver,
                panel: Panel::Brushed,
                ink: Color32::from_rgb(24, 44, 92),
                knob: KnobFinish::Silver,
                pointer: Color32::from_rgb(28, 44, 88),
                pointer_shadow: Color32::from_rgb(244, 246, 250),
                jewel: JewelTint::Blue,
                piping: Color32::from_rgb(214, 216, 218),
                accent: Color32::from_rgb(48, 86, 156),
            },
            Era::British => Look {
                era: self,
                weave: Weave::Levant,
                tolex: [0.040, 0.038, 0.036],
                cloth: Cloth::Checker,
                panel: Panel::Gold,
                ink: Color32::from_rgb(22, 18, 11),
                knob: KnobFinish::Gold,
                pointer: Color32::from_rgb(28, 22, 12),
                pointer_shadow: Color32::from_rgb(244, 224, 160),
                jewel: JewelTint::Red,
                piping: Color32::from_rgb(196, 166, 92),
                accent: Color32::from_rgb(188, 154, 76),
            },
            Era::Modern => Look {
                era: self,
                weave: Weave::Vinyl,
                tolex: [0.052, 0.052, 0.055],
                cloth: Cloth::Silver,
                panel: Panel::Gunmetal,
                ink: Color32::from_rgb(226, 226, 222),
                knob: KnobFinish::BlackChrome,
                pointer: Color32::from_rgb(240, 240, 238),
                pointer_shadow: Color32::from_rgb(30, 30, 32),
                jewel: JewelTint::Blue,
                piping: Color32::from_rgb(98, 100, 104),
                accent: Color32::from_rgb(150, 154, 160),
            },
            Era::Metal => Look {
                era: self,
                weave: Weave::Levant,
                tolex: [0.032, 0.031, 0.031],
                cloth: Cloth::Black,
                panel: Panel::Gunmetal,
                ink: Color32::from_rgb(238, 232, 228),
                knob: KnobFinish::BlackChrome,
                pointer: Color32::from_rgb(236, 62, 48),
                pointer_shadow: Color32::from_rgb(28, 26, 26),
                jewel: JewelTint::Red,
                piping: Color32::from_rgb(184, 66, 52),
                accent: Color32::from_rgb(226, 70, 54),
            },
        }
    }
}

pub struct Look {
    pub era: Era,
    pub weave: Weave,
    pub tolex: [f32; 3],
    pub cloth: Cloth,
    pub panel: Panel,
    pub ink: Color32,
    pub knob: KnobFinish,
    /// Line on the knob's rotating pointer.
    pub pointer: Color32,
    /// Contrasting line on the other half of the pointer.
    pub pointer_shadow: Color32,
    pub jewel: JewelTint,
    pub piping: Color32,
    pub accent: Color32,
}

impl Look {
    pub fn tag(&self) -> String {
        format!("{} {}", self.era.label(), self.era.years())
    }
}

/// Pick the cabinet a model id was built for.
pub fn for_model(model: u16) -> Look {
    let era = match model {
        0x007c | 0x0067 | 0x0064 => Era::Tweed,
        0x00ea => Era::Brownface,
        0x0061 | 0x0079 | 0x005e => Era::British,
        0x0072 => Era::Modern,
        0x006d => Era::Metal,
        0x005d => Era::Silverface,
        _ => Era::Blackface,
    };
    era.look()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Weave {
    /// Fine pebbled vinyl, the blackface and modern covering.
    Vinyl,
    /// Lacquered basket-weave tweed.
    Tweed,
    /// Coarse levant grain, as used on British cabinets.
    Levant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cloth {
    Silver,
    Oxblood,
    Wheat,
    Checker,
    Black,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Black,
    Brushed,
    Gold,
    Brown,
    Gunmetal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KnobFinish {
    BlackChrome,
    Cream,
    Silver,
    Gold,
}

impl KnobFinish {
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JewelTint {
    Red,
    Blue,
    Amber,
}

impl JewelTint {
    pub fn index(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_voice_gets_the_cabinet_it_was_built_for() {
        for (model, era) in [
            (0x007c, Era::Tweed),
            (0x0067, Era::Tweed),
            (0x0064, Era::Tweed),
            (0x00ea, Era::Brownface),
            (0x0053, Era::Blackface),
            (0x006a, Era::Blackface),
            (0x0075, Era::Blackface),
            (0x005d, Era::Silverface),
            (0x0061, Era::British),
            (0x0079, Era::British),
            (0x005e, Era::British),
            (0x0072, Era::Modern),
            (0x006d, Era::Metal),
        ] {
            assert_eq!(for_model(model).era, era, "model 0x{model:04x}");
        }
    }

    #[test]
    fn the_badge_names_the_era_and_its_years() {
        let tag = Era::Blackface.look().tag();
        assert!(tag.contains("Blackface"));
        assert!(tag.contains("1964"));
    }
}
