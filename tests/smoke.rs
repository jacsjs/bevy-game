mod common;

#[test]
fn boots_and_ticks() {
    let mut app = common::app_headless();
    for _ in 0..3 { app.update(); }
}
