use eframe::egui::Color32;
use egui_lucide::{Lucide, LucideIcon};

pub fn named(name: &str) -> Lucide {
    match name {
        "video" => Lucide::Clapperboard,
        "upscale" => Lucide::Expand,
        "interpolate" => Lucide::Blend,
        "decompress" => Lucide::ScanLine,
        "denoise" => Lucide::AudioWaveform,
        "deblur" => Lucide::Focus,
        "slomo" => Lucide::Gauge,
        "output" => Lucide::FolderOutput,
        "search" => Lucide::Search,
        "check" => Lucide::Check,
        "benchmark" => Lucide::Gauge,
        _ => page(name),
    }
}

pub fn page(name: &str) -> Lucide {
    match name {
        "home" => Lucide::House,
        "process" => Lucide::WandSparkles,
        "download" => Lucide::Download,
        "library" => Lucide::Search,
        "benchmark" => Lucide::Gauge,
        "runtime" => Lucide::Cpu,
        "python" => Lucide::CodeXml,
        _ => Lucide::Settings2,
    }
}

pub fn styled(name: &str, color: Color32, size: f32) -> LucideIcon {
    named(name).size(size).stroke_width(1.75).color(color)
}
