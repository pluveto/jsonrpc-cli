

#[test]
// use `DEV_SERVER=1 cargo test --package jsonrpc_cli --test serve -- test_serve_dev_only --exact --nocapture`
fn test_serve_dev_only() {
    use jsonrpc_http_server::jsonrpc_core::{IoHandler, Params, Value};
    use jsonrpc_http_server::ServerBuilder;

    let dev_server = std::env::var("DEV_SERVER");
    if dev_server.is_err() {
        return;
    }
    if dev_server.unwrap() != "1" {
        return;
    }

    fn main() {
        let mut io = IoHandler::default();
        io.add_method("say_hello", |_params: Params| async {
            Ok(Value::String("hello".to_owned()))
        });

        // add [u64, u64] -> u64
        io.add_method("add", |params: Params| async move {
            let params = params.parse::<(u64, u64)>()?;
            Ok(Value::Number((params.0 + params.1).into()))
        });

        // vector_product {"a": [1,2,3], "b": [4,5,6]} -> [4,10,18]
        io.add_method("vector_product", |params: Params| async move {
            let params = params.parse::<Value>()?;
            let a = params["a"].as_array().ok_or("a is not an array");
            if a.is_err() {
                return Err(jsonrpc_core::Error::invalid_params(a.err().unwrap()));
            }

            let b = params["b"].as_array().ok_or("b is not an array");
            if b.is_err() {
                return Err(jsonrpc_core::Error::invalid_params(b.err().unwrap()));
            }

            let a = a.unwrap();
            let b = b.unwrap();
            if a.len() != b.len() {
                return Err(jsonrpc_core::Error::invalid_params(
                    "a and b must have the same length",
                ));
            }
            let mut result = Vec::new();
            for (a, b) in a.iter().zip(b.iter()) {
                let a = a.as_u64().ok_or("a is not a u64");
                if a.is_err() {
                    return Err(jsonrpc_core::Error::invalid_params(a.err().unwrap()));
                }

                let b = b.as_u64().ok_or("b is not a u64");
                if b.is_err() {
                    return Err(jsonrpc_core::Error::invalid_params(b.err().unwrap()));
                }

                let a = a.unwrap();
                let b = b.unwrap();
                result.push(Value::Number((a * b).into()));
            }
            Ok(Value::Array(result))
        });

        let host = "127.0.0.1:3030";
        let url = format!("http://{}", host);
        let server = ServerBuilder::new(io)
            .threads(1)
            .start_http(&host.parse().unwrap())
            .unwrap();

        println!("Dev server listening on {url}");
        println!("Press Ctrl-C to stop");
        println!("---");
        println!("Test with:");
        println!("cargo run -- -e {url} add 1,2 | jq");
        println!(r#"cargo run -- -e {url} vector_product '{{"a": [1,2,3], "b": [4,5,6]}}' | jq"#);

        server.wait();
    }

    main();
}
