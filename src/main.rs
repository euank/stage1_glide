#![feature(proc_macro)]

#[macro_use]
extern crate log;
extern crate env_logger;
extern crate docopt;
extern crate rustc_serialize;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate libc;

mod appc;

use appc::PodManifest;
use std::vec::Vec;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Write;

fn main() {
    // Split based on our arg0 so we can get away with only packaging one binary.
    let bin = std::env::args().next().unwrap();
    let args = std::env::args().collect();

    let bin_name = std::path::Path::new(&bin).file_name().unwrap().to_str().unwrap();
    match bin_name {
        "init" => {
            std::process::exit(init(args));
        }
        "gc" => {
            // we don't create any mounts or crazyness, so gc for us is easy!
            std::process::exit(0);
        }
        "enter" => {
            println!("enter not supported");
            std::process::exit(254);
        }

        _ => panic!("called with invalid entrypoint '{}'", bin),
    };
}

const INIT_STR: &'static str = "
Usage: init [options] <uuid>

Options:
    --debug=<bool>           debug
    --net=<str>              net, cannot be specified
    --mds-token=<token>      metadata service, cannot be specified
    --interactive            don't specify me
    --local-config=<path>    don't specify me
    --private-users=<shift>  don't specify me
    --hostname=<hostname>    don't specify me
";

#[derive(RustcDecodable,Debug)]
struct InitArgs {
    flag_debug: bool,
    flag_net: String,
    flag_mds_token: String,
    flag_interactive: bool,
    flag_local_config: String,
    flag_private_users: String,
    flag_hostname: String,
    arg_uuid: String,
}


fn init(args: Vec<String>) -> i32 {
    let args: InitArgs = docopt::Docopt::new(INIT_STR)
        .and_then(|d| d.argv(args).decode())
        .unwrap();
    println!("init called with args: {:?}", args);

    for flag in vec![args.flag_mds_token, args.flag_private_users, args.flag_hostname] {
        if !flag.is_empty() {
            warn!("unsupported flag {} specified", flag);
            return 254;
        }
    }

    {
        let mut logger = env_logger::LogBuilder::new();
        if args.flag_debug {
            logger.filter(None, log::LogLevelFilter::Debug);
        }
        logger.init().err().map(|e| {
            println!("unable to init logger: {}", e);
        });
    }

    let manifest_file = std::fs::File::open("pod").unwrap();
    let manifest: PodManifest = serde_json::from_reader(manifest_file).unwrap();
    if manifest.apps.len() != 1 {
        warn!("pod must have a single application");
        return 254;
    }

    // Unwrap is safe due to length check above
    let runtime_app = manifest.apps.first().unwrap();
    let ref app = runtime_app.app;
    let app_root = PathBuf::new()
        .join(".")
        .join("stage1")
        .join("rootfs")
        .join("opt")
        .join("stage2")
        .join(runtime_app.name.clone())
        .join("rootfs");
    let app_root_path = app_root.as_path();

    let mut exec = app.exec.clone();
    debug!("running cmd: {:?}", exec);
    let args = exec.split_off(1);
    let default_exec_cmd = "sh".to_string();
    let exec_cmd: &String = {
        exec.first().unwrap_or(&default_exec_cmd)
    };
    let mut exec_cmd_path = Path::new(exec_cmd);
    if exec_cmd_path.is_absolute() {
        exec_cmd_path = exec_cmd_path.strip_prefix("/").unwrap();
    }

    let my_pid = unsafe { libc::getpid() };
    let mut pid_file = match File::create("pid") {
        Ok(f) => f,
        Err(e) => {
            println!("unable to create pid file: {}", e);
            return 254;
        }
    };
    match pid_file.write_all(my_pid.to_string().as_bytes()) {
        Err(e) => {
            println!("unable to write pid file: {}", e);
            return 254;
        }
        _ => {}
    };


    // TODO(euank): environment variables, path, etc
    debug!("my command is {:?} with args {:?} and root path {:?}",
           exec_cmd_path,
           args,
           app_root_path);
    let mut cmd = Command::new(exec_cmd_path);
    cmd.args(&args);
    cmd.current_dir(app_root_path);
    let err = cmd.exec();
    println!("error executing entrypoint: {}", err);
    return 254;
}
