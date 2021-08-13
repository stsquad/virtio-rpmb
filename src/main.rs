/*
 * vhost-user-rpmb daemon
 *
 * (C)opyright 2020 Linaro
 * Author: Alex Bennée <alex.bennee@linaro.org>
 */

#[macro_use]
extern crate clap;
use clap::App;

use log::*;

use std::process::exit;
use std::path::Path;
use std::sync::{Arc, RwLock};

use vhost_user_backend::{VhostUserDaemon};
use vhost::vhost_user::{Listener};
use vhost_user_rpmb::rpmb::RpmbBackend;
use vhost_user_rpmb::vhu_rpmb::VhostUserRpmb;

fn main() -> Result<(), String> {
    let yaml = load_yaml!("cli.yaml");
    let cmd_args = App::from_yaml(yaml).get_matches();

    if cmd_args.is_present("print_cap") {
        println!("{{");
        println!("  \"type\": \"block\"");
        println!("}}");
        exit(0);
    }

    stderrlog::new().module(module_path!())
        .verbosity(cmd_args.occurrences_of("verbose") as usize)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    let flash_path = Path::new(cmd_args.value_of("flash_path").unwrap());
    if !flash_path.exists() {
            println!("Please specify a valid --flash-path for the \
                      flash image");
            exit(1);
    }

    let rpmb = match RpmbBackend::new(&flash_path) {
        Ok(s) => s,
        Err(e) => {
            println!("Can't open flash image {}: {}", flash_path.display(), e);
            exit(-1);
        }
    };

    let socket = match cmd_args.value_of("socket") {
        Some(path) => path,
        None => {
            error!("Failed to retrieve vhost-user socket path");
            exit(-1);
        }
    };

    let listener = Listener::new(socket, true).unwrap();

    let backend = Arc::new(RwLock::new(VhostUserRpmb::new(rpmb).unwrap()));

    let mut daemon =
        VhostUserDaemon::new(String::from("vhost-user-rpmb-backend"), backend.clone()).unwrap();

    daemon.start(listener).unwrap();
    daemon.wait().unwrap();

    Ok(())
}
