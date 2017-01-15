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
    debug!("init called with args: {:?}", args);

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

    let manifest_file = match std::fs::File::open("pod") {
        Ok(file) => file,
        Err(e) => {
            println!("unable to open pod: {}", e);
            return 254;
        }
    };
    let manifest: PodManifest = serde_json::from_reader(manifest_file).unwrap();
    if manifest.apps.len() != 1 {
        warn!("pod must have a single application");
        return 254;
    }

    // Unwrap is safe due to length check above
    let runtime_app = manifest.apps.first().unwrap();
    let pod_root = std::env::current_dir().unwrap();
    let app = &runtime_app.app;
    let app_root = PathBuf::new()
        .join(pod_root)
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
    let exec_cmd = {
        exec.first().unwrap_or(&default_exec_cmd)
    };

    let app_path = mangle_env(app_root_path,
                              app.environment
                                  .iter()
                                  .find(|kv| kv.name == "PATH")
                                  .map(|kv| kv.value.clone())
                                  .unwrap_or("/sbin:/usr/sbin:/bin:/usr/bin:/usr/local/sbin:\
                                              /usr/local/bin"
                                      .to_string()));

    let exec_cmd_absolute_path =
        match resolve_in_root_with_path(app_root_path, app_path.clone(), exec_cmd.clone()) {
            Some(p) => p,
            None => {
                println!("entrypoint not found in PATH");
                return 254;
            }
        };
    let exec_cmd = if Path::new(&exec_cmd.clone()).is_absolute() {
        path_in_root(app_root_path, exec_cmd.to_string())
    } else {
        exec_cmd.clone()
    };

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

    // So, because of the ld.so.cache we have to do fun things here.  If it's a dynamic library, we
    // need to skip the cache or it won't start because of the LD_LIBRARY_PATH overriding.. Here we
    // gooo!
    let (exec_cmd_path, args) = ld_bust_args(app_root_path,
                                             exec_cmd_absolute_path,
                                             exec_cmd.clone(),
                                             args);
    let exec_cmd_path = Path::new(&exec_cmd_path);
    let mut cmd = Command::new(exec_cmd_path);
    cmd.args(&args);
    cmd.current_dir(app_root_path);
    cmd.env_clear();
    match std::env::var("TERM") {
        Ok(val) => {
            cmd.env("TERM", val);
        }
        _ => {}
    };
    // TODO: parse /etc/ld.so.conf to actually have a complete value here
    let possible_ld_values = "/lib64:/lib/x86_64-linux-gnu:/lib:/lib32:/lib64:/usr/lib64:/usr/lib:\
                              /usr/lib32:/usr/lib/x86_64-linux-gnu";
    cmd.env("LD_LIBRARY_PATH",
            mangle_env(app_root_path, possible_ld_values.to_string()));


    let mangle_paths = vec!["LD_LIBRARY_PATH"];
    for env_pair in &app.environment {
        let name = env_pair.name.clone();
        let mut val = env_pair.value.clone();
        if mangle_paths.iter().any(|k| k.to_string() == name) {
            debug!("mangling {}", name);
            val = mangle_env(app_root_path, val);
        }
        cmd.env(name, val);
    }
    cmd.env("PATH", app_path);

    debug!("my command is {:?} with args {:?} and root path {:?}",
           exec_cmd_path,
           args,
           app_root_path);
    let err = cmd.exec();
    println!("error executing entrypoint: {}", err);
    return 254;
}

// mangle_env will take a colon-separated string (such as a PATH environment variable) and prefix
// each component with, well, prefix.
fn mangle_env(prefix: &Path, s: String) -> String {
    s.split(':')
        .map(|part| path_in_root(prefix, part.to_string()))
        .fold("".to_string(), |x, y| {
            if x == "" {
                // First time, skip the ':'
                y
            } else {
                format!("{}:{}", x, y)
            }
        })
}

fn ld_bust_args(app_root: &Path,
                entrypoint_absolute_path: String,
                entrypoint: String,
                args: Vec<String>)
                -> (String, Vec<String>) {
    // TODO there's definitely a better way to do this
    let ld_so_paths = vec!["/lib/ld-linux.so.2",
                           "/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
                           "/lib64/ld-linux-x86-64.so.2",
                           "/lib32/ld-linux.so.2"];

    let ld_so_path = ld_so_paths.iter()
        .map(|el| path_in_root(app_root, el.to_string()))
        .find(|el| Path::new(el).exists());

    let ld_so_path = match ld_so_path {
        None => {
            debug!("could not find any ld-linux.so in rootfs");
            return (entrypoint, args);
        }
        Some(path) => path,
    };

    debug!("checking {}", entrypoint_absolute_path.clone());
    let mut ld_cmd = Command::new(ld_so_path.clone());
    ld_cmd.arg("--verify");
    ld_cmd.arg(entrypoint_absolute_path.clone());
    match ld_cmd.output() {
        Ok(s) => {
            if s.status.success() {
                // Mangle away!
                debug!("ld.so mangling because verify told us we could");
                let mut new_args = Vec::new();
                new_args.push("--inhibit-cache".to_string());
                new_args.push(entrypoint_absolute_path);
                // TODO transform ep and args as references, not cloning
                new_args.extend(args.iter().cloned());
                (ld_so_path.to_string(), new_args)
            } else {
                debug!("ld.so verify told us not to mangle, hopefully static");
                (entrypoint, args)
            }
        }
        Err(e) => {
            debug!("error running {}: {}", ld_so_path, e);
            (entrypoint, args)
        }
    }
}

// TODO this should resolve symlinks correctly
fn path_in_root(root: &Path, path: String) -> String {
    let mut inroot_path = Path::new(&path);
    if inroot_path.is_absolute() {
        inroot_path = inroot_path.strip_prefix("/").unwrap();
    }
    PathBuf::from(root).join(inroot_path).into_os_string().into_string().unwrap()
}

fn resolve_in_root_with_path(root: &Path, path: String, cmd: String) -> Option<String> {
    let cmd_copy = cmd.clone();
    let p = Path::new(&cmd_copy);
    if p.is_absolute() {
        return Some(path_in_root(root, cmd));
    };

    let respath = path.split(':')
        .map(|el| {
            let pir = Path::new(el);
            pir.join(cmd.clone())
        })
        .find(|el| el.exists());
    respath.map(|el| el.into_os_string().into_string().unwrap())
}
