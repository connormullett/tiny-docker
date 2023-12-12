use std::{
    env::set_current_dir,
    error::Error,
    ffi::CString,
    fs::File,
    io::BufReader,
    os::{fd::IntoRawFd, unix::fs::chroot},
    path::Path,
    process,
};

use nix::{
    sys::wait::waitpid,
    unistd::{execvp, fork, write, ForkResult},
};

use errno::errno;

use flate2::bufread::GzDecoder;
use libc::{CLONE_NEWCGROUP, CLONE_NEWIPC, CLONE_NEWNET, CLONE_NEWNS, CLONE_NEWPID, CLONE_NEWUTS};
use tar::Archive;

fn main() -> Result<(), Box<dyn Error>> {
    let path = "rootfs.tar.gz";
    let root_path = Path::new("./rootfs");

    // unpack rootfs
    if !root_path.exists() {
        let tar_gz = File::open(path)?;
        let reader = BufReader::new(tar_gz);
        println!("decoing tarball");
        let tar = GzDecoder::new(reader);
        let mut archive = Archive::new(tar);
        println!("unpacking archive");
        archive.unpack(".")?;
    }

    // copy file
    println!("copying hello world script");
    std::fs::copy("hello.sh", "./rootfs/hello.sh")?;

    println!("id :: {}", process::id());

    // setns
    let procns_path = format!("/proc/{}/ns/pid", process::id());
    let fd = File::open(procns_path)?;

    println!("setting namespace");
    let err = unsafe { libc::setns(fd.into_raw_fd(), CLONE_NEWPID) };
    if err == -1 {
        if err == -1 {
            let e = errno();
            panic!("{e}");
        }
    }

    // unshare
    let flags =
        CLONE_NEWNS | CLONE_NEWCGROUP | CLONE_NEWIPC | CLONE_NEWNET | CLONE_NEWUTS | CLONE_NEWPID;

    println!("unsharing from parent namespace");
    unsafe {
        let error = libc::unshare(flags);
        if error == -1 {
            let e = errno();
            panic!("{e}");
        }
    }

    // fork
    println!("forking process");
    let program = CString::new("ls")?;
    let args = [CString::new("ls")?, CString::new("-l")?];
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => {
            println!(
                "Continuing execution in parent process, new child has pid: {}",
                child
            );
            waitpid(child, None).unwrap();
        }
        Ok(ForkResult::Child) => {
            // Unsafe to use `println!` (or `unwrap`) here. See Safety.
            write(
                libc::STDOUT_FILENO,
                "created new child process\n".as_bytes(),
            )
            .ok();

            // chroot
            println!("changing root to rootfs");
            chroot(root_path)?;
            println!("entering rootfs");
            set_current_dir(root_path)?;

            // exec
            execvp(&program, &args).ok();

            write(libc::STDOUT_FILENO, "OK\n".as_bytes()).ok();
        }
        Err(_) => println!("Fork failed"),
    };

    Ok(())
}
