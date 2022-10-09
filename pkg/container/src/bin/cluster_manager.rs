// Binary executed by the manager workers in the cluster which start jobs and
// watch over workers.

extern crate common;
extern crate container;

use common::errors::*;

fn main() -> Result<()> {
    container::manager_main()
}
