// jsonrpc-cli
// A jsonrpc command line tool for development and testing

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};
use jsonrpc_core::types::{Id, Version};
use serde_json::{json, Value};
use std::fs;
use ureq::Error;

fn main() -> Result<()> {
    // Check if endpoint is set from environment variable
    let endpoint_env = std::env::var("JSONRPC_ENDPOINT");
    let has_endpoint_set = endpoint_env.is_ok();
    // Parse command line arguments
    let cmd = Command::new("jsonrpc-cli")
        .version("0.1.0")
        .author("Zijing Zhang. <pluvet@foxmail.com>")
        .about("A jsonrpc command line utility for development and testing")
        .arg(
            Arg::new("endpoint")
                .short('e')
                .long("endpoint")
                .value_name("ENDPOINT")
                .help("jsonrpc server endpoint")
                .required(has_endpoint_set),
        )
        .arg(
            Arg::new("method")
                .value_name("METHOD")
                .help("jsonrpc method"),
        )
        .arg(
            Arg::new("params")
                .value_name("PARAMS")
                .help("jsonrpc params")
                .num_args(0..),
        )
        .arg(
            Arg::new("rpc-version")
                .short('v')
                .long("rpc-version")
                .default_value("2.0")
                .help("jsonrpc version, default \"2.0\"."),
        )
        .arg(
            Arg::new("id")
                .short('i')
                .long("id")
                .value_name("ID")
                .default_value("null()")
                .help("jsonrpc id, e.g. \"test\", \"int(1)\", \"null()\""),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(ArgAction::SetTrue)
                .help("verbose output"),
        );

    let matches = cmd.get_matches();

    // Get the endpoint
    let endpoint_w = matches.get_one::<String>("endpoint");
    if endpoint_w.is_none() && !has_endpoint_set {
        return Err(anyhow::anyhow!("endpoint is not set")).context(
            "Cannot find valid endpoint \n\
             Please set the endpoint with -e or --endpoint option \
             or JSONRPC_ENDPOINT environment variable",
        );
    }

    let endpoint: String;
    if let Some(endpoint_w) = endpoint_w {
        endpoint = endpoint_w.to_owned();
    } else {
        endpoint = endpoint_env.unwrap();
    }

    let method = matches.get_one::<String>("method");
    let version = matches.get_one::<String>("rpc-version").unwrap();
    let id = matches.get_one::<String>("id").unwrap();
    let params: Vec<_> = matches
        .get_many::<String>("params")
        .unwrap_or_default()
        .map(|x| x.as_str())
        .collect();

    let verbose = matches.get_one::<bool>("debug").is_some();

    // Construct the jsonrpc request
    let request = build_request(method.map(|x| x.as_str()), version, id, params)?;

    verbose_log(verbose, format!("sending request to {}", endpoint));
    verbose_log(verbose, "request payload:".to_string());
    verbose_log_value(verbose, &request);

    // Send the request and print the response
    verbose_log(verbose, "response:".to_string());
    send_request(&endpoint, request)?;
    verbose_log(verbose, "done".to_string());
    Ok(())
}

fn verbose_log(flag: bool, msg: String) {
    if flag {
        // to be compatible with jq
        println!("\"[verbose] {}\"", msg);
    }
}

fn verbose_log_value(flag: bool, msg: &Value) {
    if flag {
        // to be compatible with jq
        println!("{}", msg);
    }
}

// Build the jsonrpc request from the arguments
fn build_request(
    method: Option<&str>,
    version: &str,
    id: &str,
    params: Vec<&str>,
) -> Result<Value> {
    if method.is_none() {
        return Ok(json!({
            "jsonrpc": version,
            "id": parse_simple_expr(id)?,
        }));
    }

    // Parse the version
    let version = match version {
        "1.0" => panic!("jsonrpc 1.0 is not supported yet"),
        "2.0" => Version::V2,
        _ => panic!("invalid jsonrpc version: {}", version),
    };

    // Parse the id
    let id = parse_simple_expr(id)?;

    // Parse the params
    let params = parse_params(params);
    if params.is_err() {
        return params.with_context(|| "invalid params");
    }

    // Return the jsonrpc request
    Ok(json!({
        "jsonrpc": version,
        "method": method.unwrap(),
        "params": params.unwrap(),
        "id": id,
    }))
}

// Parse the id argument
// expr: int(123), null(), test
fn parse_simple_expr(expr: &str) -> Result<Id> {
    let re = regex::Regex::new(r"^(int|null)\((.*)\)$").unwrap();
    let caps = re.captures(expr);
    if caps.is_none() {
        return Ok(Id::Str(expr.to_string()));
    }
    let caps = caps.unwrap();
    let name = caps.get(1).unwrap().as_str();
    let arg = caps.get(2).unwrap().as_str();
    match name {
        "int" => {
            let value = arg.parse::<u64>().expect("invalid int argument");
            Ok(Id::Num(value))
        }
        "null" => {
            if !arg.is_empty() {
                panic!("invalid null argument");
            }
            Ok(Id::Null)
        }
        _ => panic!("unsupported function: {}", name),
    }
}

fn extract_key_value<'a>(stripped: &'a str, next_param: Option<&'a str>) -> (&'a str, &'a str) {
    let parts = stripped.splitn(2, '=').collect::<Vec<&str>>();
    match parts.len() {
        1 => (
            parts[0],
            next_param.unwrap_or_else(|| panic!("missing value for param {}", stripped)),
        ),
        2 => (parts[0], parts[1]),
        _ => panic!("invalid long option: {}", stripped),
    }
}

// Parse the params argument
fn parse_params(params: Vec<&str>) -> Result<Value> {
    if params.is_empty() {
        return Ok(Value::Null);
    }

    // Get the params as a vector of strings
    // Check if the first param is a JSON string
    if params[0].starts_with('{') || params[0].starts_with('[') {
        return serde_json::from_str(params[0])
            .with_context(|| format!("invalid JSON string: {}", params[0]));
    }

    // Check if the first param is a JSON file
    if params[0].starts_with('@') {
        let path = &params[0][1..];
        let content = fs::read_to_string(path)?;
        return serde_json::from_str(&content).with_context(|| "invalid JSON file");
    }

    // Check if the first param is an array
    if params[0].contains(',') {
        let items = params[0].split(',');
        let values = items.map(serde_json::from_str::<Value>);
        let errors = values.clone().filter(|value| value.is_err());
        if errors.clone().count() > 0 {
            let formated_erros = errors
                .map(|error| error.unwrap_err().to_string())
                .collect::<Vec<String>>()
                .join("\n");
            return Err(anyhow::anyhow!("invalid JSON array")).with_context(|| formated_erros);
        }
        let collected = values.map(|value| value.unwrap()).collect::<Vec<Value>>();
        return Ok(Value::Array(collected));
    }

    // if no -- or - or = in params, treat it as array param
    if !params[0].contains("--") && !params[0].contains('-') && !params[0].contains('=') {
        let values = params
            .iter()
            .map(|item| serde_json::from_str::<Value>(item));
        let errors = values.clone().filter(|value| value.is_err());
        if errors.clone().count() > 0 {
            let formated_erros = errors
                .map(|error| error.unwrap_err().to_string())
                .collect::<Vec<String>>()
                .join("\n");
            return Err(anyhow::anyhow!("invalid JSON array")).with_context(|| formated_erros);
        }
        let collected = values.map(|value| value.unwrap()).collect::<Vec<Value>>();
        return Ok(Value::Array(collected));
    }

    // Assume the params are key-value pairs
    let mut object = serde_json::Map::new();
    let mut param_it = params.iter();
    while let Some(param) = param_it.next() {
        // Check if param is a long option
        // like: --key=value or --key value
        if let Some(stripped) = param.strip_prefix("--") {
            // Split the param by equal sign
            let (key, value) = extract_key_value(stripped, param_it.next().copied());
            object.insert(key.to_string(), Value::String(value.to_string()));
        // Check if param is a short 'p'ion
        } else if let Some(stripped) = param.strip_prefix('-') {
            let (key, value) = extract_key_value(stripped, param_it.next().copied());
            object.insert(key.to_string(), Value::String(value.to_string()));

        // Otherwise, assume param is a function call
        } else {
            let parts = param.splitn(2, '(').collect::<Vec<&str>>();
            let (name, arg) = match parts.len() {
                // If only one part, use it as name and empty string as argument
                1 => (parts[0], ""),
                // If two parts, use them as name and argument
                2 => (parts[0], parts[1]),
                // Otherwise, panic
                _ => panic!("invalid function call: {}", param),
            };
            // Check if the argument ends with parenthesis
            if !arg.ends_with(')') {
                panic!("invalid function call: {}", param);
            }
            // Remove the last parenthesis from the argument
            let arg = &arg[..arg.len() - 1];
            parse_function_call(name, arg, &mut object);
        }
    }
    // Return the object as a value
    Ok(Value::Object(object))
}

// Parse the function call and insert the result into the object
fn parse_function_call(name: &str, arg: &str, object: &mut serde_json::Map<String, Value>) {
    // Match the function name
    match name {
        // int function
        "int" => {
            // Parse the argument as i64
            let value = arg.parse::<i64>().expect("invalid int argument");
            // Insert the value as number into the object
            object.insert(name.to_string(), Value::Number(value.into()));
        }
        // null function
        "null" => {
            // Check if the argument is empty
            if !arg.is_empty() {
                panic!("invalid null argument");
            }
            // Insert the value as null into the object
            object.insert(name.to_string(), Value::Null);
        }
        // Other functions are not supported
        _ => panic!("unsupported function: {}", name),
    }
}

// Send the jsonrpc request and print the response
fn send_request(endpoint: &str, request: Value) -> Result<()> {
    // Send the request as a POST request with JSON content type
    let response = ureq::post(endpoint)
        .set("Content-Type", "application/json")
        .send_string(&request.to_string());

    // Print the response
    match response {
        Ok(response) => {
            let body = response
                .into_string()
                .with_context(|| "failed to read response body".to_string())?;
            println!("{}", body);
        }
        Err(Error::Status(code, response)) => {
            let body = response
                .into_string()
                .with_context(|| format!("failed to read response body with code={code}"))?;
            return Err(anyhow::anyhow!("{}: {}", code, body).context("response status not ok"));
        }
        Err(Error::Transport(transport)) => {
            return Err(anyhow::anyhow!("transport error: {}", transport));
        }
    }

    Ok(())
}
