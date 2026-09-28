//! Affiche les événements gilrs bruts (diagnostic de mapping).
fn main() {
    #[cfg(feature = "gamepad")]
    {
        let mut g = gilrs::Gilrs::new().expect("gilrs");
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < 12 {
            while let Some(ev) = g.next_event() {
                println!("{:5.2}s {:?}", t0.elapsed().as_secs_f64(), ev.event);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
