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

mod appc;

use appc::PodManifest;
use std::vec::Vec;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn main() {
    // Split based on our arg0 so we can get away with only packaging one binary.
    let bin = std::env::args().next().unwrap();
    let args = std::env::args().skip(1).collect();

    println!("{:?}", args);

    let bin_name = std::path::Path::new(&bin).file_name().unwrap().to_str().unwrap();
    match bin_name {
        "init" => init(args),

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


fn init(args: Vec<String>) {
    let args: InitArgs = docopt::Docopt::new(INIT_STR)
        .and_then(|d| d.argv(args).decode())
        .unwrap();
    println!("init called with args: {:?}", args);

    for flag in vec![args.flag_mds_token, args.flag_private_users, args.flag_hostname] {
        if !flag.is_empty() {
            panic!("unsupported flag specified");
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
    let app = match manifest.apps.first() {
        Some(app) => app,
        None => {
            println!("pod must have a single application");
            return;
        }
    };
    let ref app = app.app;

    let mut exec = app.exec.clone();
    debug!("running cmd: {:?}", exec);
    let args = exec.split_off(1);
    // TODO(euank): cmd must be normalized to be within the stage1
    // TODO(euank): environment variables, path, etc
    let mut cmd = Command::new(exec.first().unwrap());
    cmd.args(&args);
    let err = cmd.exec();
    println!("error executing entrypoint: {}", err);
}
