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
        "scene_change" => Lucide::Scissors,
        "slomo" => Lucide::Gauge,
        "output" => Lucide::FolderOutput,
        "search" => Lucide::Search,
        "check" => Lucide::Check,
        "plus" => Lucide::Plus,
        "trash" => Lucide::Trash,
        "queue" => Lucide::ListVideo,
        "folder" => Lucide::FolderOpen,
        "command" => Lucide::Command,
        "play" => Lucide::Play,
        "pause" => Lucide::Pause,
        "stop" => Lucide::Square,
        "close" => Lucide::X,
        "refresh" => Lucide::RefreshCw,
        "import" => Lucide::FileInput,
        "palette" => Lucide::Palette,
        "all" => Lucide::LayoutGrid,
        "download" => Lucide::Download,
        "storage" => Lucide::HardDrive,
        "database" => Lucide::Database,
        "shield" => Lucide::ShieldCheck,
        "key" => Lucide::KeyRound,
        "external" => Lucide::ExternalLink,
        "info" => Lucide::Info,
        "warning" => Lucide::TriangleAlert,
        "zap" => Lucide::Zap,
        "gpu" => Lucide::Gpu,
        "chip" => Lucide::Cpu,
        "memory" => Lucide::MemoryStick,
        "monitor" => Lucide::Monitor,
        "clock" => Lucide::Clock,
        "terminal" => Lucide::SquareTerminal,
        "package" => Lucide::Package,
        "wrench" => Lucide::Wrench,
        "sliders" => Lucide::SlidersHorizontal,
        "update" => Lucide::CloudDownload,
        "rocket" => Lucide::Rocket,
        "layers" => Lucide::Layers,
        "crop" => Lucide::Crop,
        "sun" => Lucide::Sun,
        "eye" => Lucide::Eye,
        "save" => Lucide::Save,
        "film" => Lucide::Film,
        "target" => Lucide::Target,
        "grid" => Lucide::Grid3x3,
        _ => page(name),
    }
}

pub fn page(name: &str) -> Lucide {
    match name {
        "home" => Lucide::House,
        "process" => Lucide::WandSparkles,
        "models" => Lucide::Boxes,
        "library" => Lucide::Globe,
        "benchmark" => Lucide::Gauge,
        "runtime" => Lucide::Cpu,
        "python" => Lucide::CodeXml,
        "sparkles" => Lucide::Sparkles,
        _ => Lucide::Settings2,
    }
}

pub fn styled(name: &str, color: Color32, size: f32) -> LucideIcon {
    named(name).size(size).stroke_width(1.75).color(color)
}
