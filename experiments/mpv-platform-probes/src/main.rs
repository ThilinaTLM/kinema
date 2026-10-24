fn main() -> std::process::ExitCode {
    // No path/config/network inputs; the only operation is destructive-to-no-user-data
    // native GPU allocation, pattern clear, and test-only readback.
    if std::env::args_os().len() != 1 {
        eprintln!("this probe accepts no arguments (native test-readback only)");
        return std::process::ExitCode::FAILURE;
    }
    match mpv_platform_probes::run_test_readback() {
        Ok(()) => {
            println!(
                "native-texture-interop test-readback PASS (4x4 BGRA8; not playback qualification)"
            );
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("test-readback FAIL: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
