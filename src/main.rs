/*
 * vhost-user-rpmb daemon
 *
 * (C)opyright 2020 Linaro
 * Author: Alex Bennée <alex.bennee@linaro.org>
 */

#[macro_use]
extern crate clap;
use clap::App;

fn main() {
    let yaml = load_yaml!("cli.yaml");
    let cmd_args = App::from_yaml(yaml).get_matches();

    println!("Hello, world!");
}
