mod sim;
mod ui;
mod verilog;

use ui::FrostbiteApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_title("Frostbite Graph Core Simulator"),
        ..Default::default()
    };

    eframe::run_native(
        "Frostbite Simulator",
        options,
        Box::new(|_cc| Ok(Box::new(FrostbiteApp::default()))),
    )
}

