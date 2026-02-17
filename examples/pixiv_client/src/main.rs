mod app;

fn main() -> std::result::Result<(), bevy_xilem::xilem::winit::error::EventLoopError> {
    app::run()
}
