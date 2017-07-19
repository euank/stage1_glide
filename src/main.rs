#![feature(proc_macro)]

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

extern crate docopt;
extern crate libc;
extern crate rustc_serialize;
extern crate walkdir;

mod appc;

use std::fs::File;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::vec::Vec;

use appc::PodManifest;
use walkdir::WalkDir;

fn main() {
    // Split based on our arg0 so we can get away with only packaging one binary.
    let bin = std::env::args().next().unwrap();
    let args = std::env::args().collect();

    let bin_name = std::path::Path::new(&bin)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    match bin_name {
        "init" => {
            match init(args) {
                Ok(_) => {
                    std::process::exit(0);
                }
                Err(e) => {
                    println!("error: {}", e);
                    std::process::exit(254);
                }
            }
        }
        "gc" => {
            // we don't create any mounts or crazyness, so gc for us is easy!
            std::process::exit(0);
        }
        "enter" => {
            warn!("enter not supported");
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

#[derive(RustcDecodable, Debug)]
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


fn init(args: Vec<String>) -> Result<(), String> {
    let args: InitArgs = docopt::Docopt::new(INIT_STR)
        .and_then(|d| d.argv(args).decode())
        .map_err(|e| format!("unable to decode arguments: {}", e))?;
    debug!("init called with args: {:?}", args);

    for flag in vec![
        args.flag_mds_token,
        args.flag_private_users,
        args.flag_hostname,
    ]
    {
        if !flag.is_empty() {
            return Err(format!("unsupported flag '{}' specified", flag));
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

    let manifest_file = std::fs::File::open("pod").map_err(|e| {
        format!("could not open pod manifest: {}", e)
    })?;

    let manifest: PodManifest = serde_json::from_reader(manifest_file).map_err(
        |e| format!("{}", e),
    )?;

    let runtime_app = manifest.apps.first().ok_or(
        "pod must have a single application",
    )?;
    let pod_root = std::env::current_dir().map_err(|e| {
        format!("could not get working dir: {}", e)
    })?;

    let stage1_root = pod_root.join("stage1").join("rootfs");

    let ldconfig_bin = PathBuf::from("ldconfig");
    debug!("using ldconfig bin: {:?}", ldconfig_bin);

    let patchelf_bin = stage1_root.join("bin").join("patchelf");

    debug!("using patchelf bin: {:?}", patchelf_bin);
    let app = &runtime_app.app;
    let app_root = stage1_root
        .join("opt")
        .join("stage2")
        .join(&runtime_app.name)
        .join("rootfs");
    let app_root_path = app_root.as_path();

    debug!("mangling symlinks");
    mangle_symlinks(app_root_path)?;
    // Figure out the right RPATH based on any ld.conf files present in the rootfs
    let ldpath_in_root = resolve_ldpath(app_root_path, &ldconfig_bin)?;

    // Run patchelf on all the files in the rootfs using patchelf
    debug!("mangling elf");
    mangle_elfs(app_root_path, &ldpath_in_root, &patchelf_bin)?;

    let mut exec = app.exec.clone();
    debug!("running cmd: {:?}", exec);
    let args = exec.split_off(1);
    let default_exec_cmd = "sh".to_string();
    let exec_cmd = {
        exec.first().unwrap_or(&default_exec_cmd)
    };

    let app_path = mangle_env(
        app_root_path,
        app.environment
            .iter()
            .find(|kv| kv.name == "PATH")
            .map(|kv| kv.value.as_ref())
            .unwrap_or(
                "/sbin:/usr/sbin:/bin:/usr/bin:/usr/local/sbin:/usr/local/bin",
            ),
    );

    let exec_cmd_absolute_path = resolve_in_root_with_path(app_root_path, &app_path, &exec_cmd)
        .ok_or("could not find entrypoint in PATH")?;

    let my_pid = unsafe { libc::getpid() };
    let mut pid_file = File::create("pid").map_err(|e| {
        format!("unable to create pid file: {}", e)
    })?;
    pid_file.write_all(my_pid.to_string().as_bytes()).map_err(
        |e| {
            format!("unable to write pid file: {}", e)
        },
    )?;

    let exec_cmd_path = Path::new(&exec_cmd_absolute_path);
    let mut cmd = Command::new(exec_cmd_path);
    cmd.args(&args);
    cmd.current_dir(app_root_path);
    cmd.env_clear();
    if let Ok(val) = std::env::var("TERM") {
        cmd.env("TERM", val);
    };

    for env_pair in &app.environment {
        cmd.env(&env_pair.name, &env_pair.value);
    }
    cmd.env("PATH", app_path);

    debug!(
        "my command is {:?} with args {:?} and root path {:?}",
        exec_cmd_path,
        args,
        app_root_path
    );
    let err = cmd.exec();
    // code never reached on success; we've exec'd away
    println!("error executing entrypoint: {}", err);
    Ok(())
}

// mangle_env will take a colon-separated string (such as a PATH environment variable) and prefix
// each component with, well, prefix.
fn mangle_env(prefix: &Path, s: &str) -> String {
    s.split(':')
        .map(|part| {
            path_in_root(prefix, part).to_string_lossy().to_string()
        })
        .fold("".to_string(), |x, y| {
            if x == "" {
                // First time, skip the ':'
                y
            } else {
                format!("{}:{}", x, y)
            }
        })
}

// mangle_symlinks re-points all the symlinks under a given rootfs to point to the correct location
// within that rootfs.
fn mangle_symlinks(rootfs: &Path) -> Result<(), String> {
    for symlink in WalkDir::new(rootfs).into_iter().filter_map(|e| {
        match e {
            Ok(entry) => {
                if entry.file_type().is_symlink() {
                    //if entry.path_is_symbolic_link() {
                    debug!(
                        "is symlink: {:?}, {:?}, {}",
                        entry.path(),
                        entry.file_type(),
                        entry.file_type().is_symlink()
                    );
                    Some(entry)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    })
    {
        let link_contents = std::fs::read_link(symlink.path()).unwrap();
        let new_target = if link_contents.is_absolute() {
            path_in_root(rootfs, &link_contents.to_string_lossy())
        } else {
            link_contents
        };
        // Delete old symlink
        std::fs::remove_file(symlink.path())
            .map_err(|e| {
                panic!("could not remove {:?}: {:?}", symlink.path(), e);
            })
            .unwrap();
        std::os::unix::fs::symlink(new_target, symlink.path()).unwrap();
    }
    Ok(())
}

fn mangle_elfs(rootfs: &Path, new_rpath: &str, patchelf_bin: &Path) -> Result<(), String> {
    for elf in WalkDir::new(rootfs).into_iter().filter_map(|e| {
        // filter for valid-enough elfs
        match e {
            Ok(path) => {
                if !path.file_type().is_file() {
                    return None;
                }
                // Note, checking if something is +x here is tempting, but distros ship `.so`s that
                // aren't +x. Looking at you debian.
                // Let's just trust ldd
                let output = match Command::new("ldd")
                    .arg(path.path().to_string_lossy().to_string())
                    .output() {
                    Err(_) => {
                        return None;
                    }
                    Ok(output) => output,
                };
                if !output.status.success() {
                    return None;
                };
                let maybe_static = String::from_utf8_lossy(&output.stdout);
                let maybe_static = maybe_static.trim();
                if maybe_static == "not a dynamic executable" ||
                    maybe_static == "statically linked"
                {
                    None
                } else {
                    Some(path)
                }
            }
            Err(_) => None,
        }
    })
    {
        // Now we loop through candidate elfs
        let interp = match Command::new(patchelf_bin)
            .arg("--print-interpreter")
            .arg(elf.path())
            .output() {
            Err(e) => {
                // probably not an elf
                debug!(
                    "unable to determine interpreter for: {:?}: {:?}",
                    elf.path(),
                    e
                );
                continue;
            }
            Ok(output) => output,
        };

        if !interp.status.success() {
            debug!(
                "not patching {:?} interp: exit status {}",
                elf,
                interp.status
            );
        } else {
            let mut old_interp = String::from_utf8_lossy(&interp.stdout).to_string();
            old_interp.pop(); // remove trailing newline
            let new_interp = path_in_root(rootfs, &old_interp)
                .to_string_lossy()
                .to_string();

            debug!("patching elf: {:?}", elf.path());
            if !Command::new(patchelf_bin)
                .arg("--set-interpreter")
                .arg(new_interp)
                .arg(elf.path())
                .status()
                .map_err(|e| format!("error running patchelf interp: {}", e))?
                .success()
            {
                return Err(format!("unable to patchelf interp: {:?}", elf.path()));
            };
        }

        // Even if we don't patch interp, we should patch rpath
        if !Command::new(patchelf_bin)
            .arg("--set-rpath")
            .arg(new_rpath)
            .arg(elf.path())
            .status()
            .map_err(|e| format!("error running patchelf rpath: {}", e))?
            .success()
        {
            panic!("unable to patchelf rpath: {:?}", elf.path());
        };
    }

    Ok(())

}

// path_in_root resolves the given path within the given rootfs
// It does not do symlink resolution or validate that the given path exists
fn path_in_root(root: &Path, path: &str) -> PathBuf {
    let mut inroot_path = Path::new(&path);


    if inroot_path.is_absolute() {
        inroot_path = inroot_path.strip_prefix("/").unwrap();
    }
    PathBuf::from(root).join(inroot_path)
}

fn resolve_in_root_with_path<'a>(root: &'a Path, path: &str, cmd: &str) -> Option<PathBuf> {
    let cmd_copy = cmd.clone();
    let p = Path::new(&cmd_copy);
    if p.is_absolute() {
        return Some(path_in_root(root, &cmd));
    };

    path.split(':')
        .map(|el| {
            let pir = Path::new(el);
            pir.join(cmd.clone())
        })
        .find(|el| el.exists())
}

// Parsing /etc/ld.so.conf is actually kinda tricky; it supports some globbing even.
// The easiest solution I've got is to use ldconfig -r[oot] and point that at the rootfs in
// question. ldconfig knows how to parse those files and has a sane known output.
fn resolve_ldpath(root: &Path, ldconfig_bin: &Path) -> Result<String, String> {
    let output = Command::new(ldconfig_bin)
        .arg("-r") // root
        .arg(root.to_string_lossy().to_string())
        .arg("-N") // no-cache-build
        .arg("-X") // no symlink update
        .arg("-v") // verbose
        .output()
        .map_err(|e| format!("could not execute {:?}: {}", ldconfig_bin, e))?;

    if !output.status.success() {
        return Err(format!(
            "expected ldconfig success ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mangled_ldpath = String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| {
        if line.starts_with(|c: char| c.is_whitespace()) {
            // the ldconfig -v output format is a list of directories and files in them. The files
            // are whitespace prefixed, so this should skip them
            return None
        } else {
        // directories have a trailing ':'
            Some(path_in_root(root, line.trim_right_matches(':')).to_string_lossy().to_string())
        }
    }).collect::<Vec<String>>().join(":");

    Ok(mangled_ldpath)
}
