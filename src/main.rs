/*
 * vhost-user-rpmb daemon
 *
 * (C)opyright 2020 Linaro
 * Author: Alex Bennée <alex.bennee@linaro.org>
 */

#[macro_use]
extern crate clap;
use clap::App;

use std::process::exit;
use std::path::Path;

use vhost_user_rpmb::rpmb::RpmbBackend;

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
}
