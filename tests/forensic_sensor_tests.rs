#[test]
fn forensic_sensor_ui_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/invalid_trigger.rs");
    t.compile_fail("tests/ui/missing_target.rs");
    t.pass("tests/ui/valid_sensor.rs");
}

use d2r_macros::forensic_sensor;

#[forensic_sensor(target = "level", trigger = "always", label = "player_level")]
pub struct PlayerStats {
    pub level: u32,
    pub experience: u64,
}

#[forensic_sensor(target = "desync_count", trigger = "on_desync", label = "desync_sensor")]
pub struct DesyncTracker {
    pub desync_count: u32,
}

#[test]
fn test_sensor_dump_always() {
    unsafe {
        std::env::remove_var("D2R_FORENSIC");
        std::env::remove_var("D2R_DESYNC");
    }
    
    let stats = PlayerStats { level: 99, experience: 3000000000 };
    stats.sensor_dump();
    
    unsafe {
        std::env::set_var("D2R_FORENSIC", "1");
    }
    stats.sensor_dump();
}

#[test]
fn test_sensor_dump_on_desync() {
    let tracker = DesyncTracker { desync_count: 5 };
    
    unsafe {
        std::env::remove_var("D2R_FORENSIC");
        std::env::remove_var("D2R_DESYNC");
    }
    tracker.sensor_dump();
    
    unsafe {
        std::env::set_var("D2R_FORENSIC", "1");
    }
    tracker.sensor_dump();
    
    unsafe {
        std::env::set_var("D2R_DESYNC", "1");
    }
    tracker.sensor_dump();
}
