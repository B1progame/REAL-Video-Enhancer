use eframe::egui::Color32;
use egui_lucide::{Lucide, LucideIcon};

pub fn page(name: &str) -> Lucide {
    match name {
        "home" => Lucide::House,
        "process" => Lucide::WandSparkles,
        "download" => Lucide::Download,
        "runtime" => Lucide::Cpu,
        "python" => Lucide::CodeXml,
        _ => Lucide::Settings2,
    }
}

pub fn styled(name: &str, color: Color32, size: f32) -> LucideIcon {
    page(name).size(size).stroke_width(1.75).color(color)
}
