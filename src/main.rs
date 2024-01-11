use rs_wgpu::run;
mod model;
mod texture;

fn main() {
    tracing_subscriber::fmt::init();

    pollster::block_on(run());
}
