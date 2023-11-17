use std::net::TcpListener;

fn port_is_available(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn get_available_port() -> Option<u16> {
    (3030..65535).find(|port| port_is_available(*port))
}

#[test]
fn test_run_without_args() {
    let output = test_bin::get_test_bin("jsonrpc-cli")
        .output()
        .expect("failed to start binary");
    let output = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.contains("cannot find valid endpoint"),
        "got: {}",
        output
    );
}

#[test]
fn test_run_without_server_running() {
    let output = test_bin::get_test_bin("jsonrpc-cli")
        .arg("-e")
        .arg("http://localhost:3030")
        .arg("say_hello")
        .output()
        .expect("failed to start binary");
    let output = String::from_utf8_lossy(&output.stderr);
    assert!(output.contains("transport error"), "got: {}", output);
}

struct ServerContext {
    port: u16,
    process: std::process::Child,
}

impl ServerContext {
    fn setup() -> Self {
        // DEV_SERVER=1 cargo test --package jsonrpc-cli --test serve -- test_serve_dev_only --exact --nocapture
        let port = get_available_port().expect("failed to find available port");
        let process = std::process::Command::new("cargo")
            .env("DEV_SERVER", "1")
            .env("PORT", port.to_string())
            .arg("test")
            .arg("--package")
            .arg("jsonrpc-cli")
            .arg("--test")
            .arg("serve")
            .arg("--")
            .arg("test_serve_dev_only")
            .arg("--exact")
            .arg("--nocapture")
            .spawn()
            .expect("failed to start server");

        use std::net::TcpStream;
        use std::thread::sleep;
        use std::time::Duration;

        let interval = Duration::from_millis(500);
        let timeout = Duration::from_secs(10);
        let max_retries = (timeout.as_millis() / interval.as_millis()) as u32;
        let mut retries = 0;
        while retries < max_retries {
            match TcpStream::connect(format!("0:{port}")) {
                Ok(_) => break,
                Err(_) if retries < 30 => {
                    retries += 1;
                    sleep(interval);
                }
                Err(e) => panic!("failed to connect: {}", e),
            }
        }
        Self { port, process }
    }
}

impl Drop for ServerContext {
    fn drop(&mut self) {
        self.process.kill().expect("failed to kill server");
    }
}

#[test]
fn test_hello() {
    let port = ServerContext::setup().port;
    let output = test_bin::get_test_bin("jsonrpc-cli")
        .arg("-e")
        .arg(format!("http://127.0.0.1:{port}"))
        .arg("say_hello")
        .output()
        .expect("failed to start binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr, "", "got: {}", stderr);
    use serde_json::Value;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let actual: Value = serde_json::from_str(&stdout).expect("Failed to parse JSON");
    let expected: Value = serde_json::json!({"jsonrpc":"2.0","result":"hello","id":null});

    assert_eq!(actual, expected, "got: {}", stdout);
}
