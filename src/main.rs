/*
 * vhost-user-rpmb daemon
 *
 * (C)opyright 2020 Linaro
 * Author: Alex Bennée <alex.bennee@linaro.org>
 */

#[macro_use]
extern crate clap;
use clap::App;

use log::{debug, error, info};

use std::process::exit;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use vhost_user_backend::{VhostUserBackend, VhostUserDaemon, Vring};
use vhost_rs::vhost_user::{Listener};
use vhost_user_rpmb::rpmb::RpmbBackend;
use vhost_user_rpmb::vhu_rpmb::VhostUserRpmb;

fn main() {
    let yaml = load_yaml!("cli.yaml");
    let cmd_args = App::from_yaml(yaml).get_matches();

    if cmd_args.is_present("print_cap") {
        println!("{{");
        println!("  \"type\": \"block\"");
        println!("}}");
        exit(0);
    }

    let flash_path = Path::new(cmd_args.value_of("flash_path").unwrap());
    if !flash_path.exists() {
            error!("Please specify a valid --flash-path for the \
                      flash image");
            exit(1);
    }

    let rpmb = match RpmbBackend::new(&flash_path) {
        Ok(s) => s,
        Err(e) => {
            error!("Can't open flash image {}: {}", flash_path.display(), e);
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

    let vuh_rpmb = Arc::new(RwLock::new(
        VhostUserRpmb::new(rpmb).unwrap(),
    ));

    let mut daemon =
        VhostUserDaemon::new(String::from("vhost-user-rpmb-backend"), vuh_rpmb.clone()).unwrap();

    if let Err(e) = daemon.start(listener) {
        error!("Failed to start daemon: {:?}", e);
        exit(-1);
    }

    if let Err(e) = daemon.wait() {
        error!("Waiting for daemon failed: {:?}", e);
    }
}
