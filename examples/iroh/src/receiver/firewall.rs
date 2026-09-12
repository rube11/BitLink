// Optional Ubuntu/UFW permission request. The receiver itself stays unprivileged.

use std::net::{IpAddr, SocketAddr};

#[cfg(target_os = "linux")]
pub fn request_access(sender_ip: IpAddr, listen_address: SocketAddr) -> Result<(), String> {
    use std::io::{self, Write};
    use std::path::Path;
    use std::process::Command;

    // These paths are for Ubuntu, including Ubuntu running inside WSL.
    if !Path::new("/usr/sbin/ufw").exists() || !Path::new("/usr/bin/sudo").exists() {
        return Err(String::from(
            "Automatic firewall permission requires Ubuntu's ufw and sudo. Run without --allow-from to manage your firewall separately.",
        ));
    }

    println!();
    println!("Firewall permission request:");
    println!(
        "Allow incoming UDP from {} to {}?",
        sender_ip, listen_address
    );
    println!("This adds one UFW rule. It does not enable or disable the firewall.");
    println!("The rule stays after the app exits. Remove it after testing with:");
    println!(
        "sudo ufw delete allow in proto udp from {} to {} port {}",
        sender_ip,
        listen_address.ip(),
        listen_address.port()
    );
    println!("If you approve, sudo may ask for your administrator password in this terminal.");
    print!("Add this rule? Type yes to approve; Enter keeps the firewall unchanged: ");
    match io::stdout().flush() {
        Ok(()) => {}
        Err(error) => {
            return Err(format!(
                "Could not display the permission request: {}",
                error
            ));
        }
    }

    let mut answer = String::new();
    match io::stdin().read_line(&mut answer) {
        Ok(_) => {}
        Err(error) => return Err(format!("Could not read your answer: {}", error)),
    }
    if answer.trim() != "yes" {
        println!("Firewall unchanged. The receiver will use the existing permissions.");
        return Ok(());
    }

    // Pass validated addresses as separate arguments, never as shell commands.
    // Only ufw gets administrator privileges, not our networking process.
    let mut command = Command::new("/usr/bin/sudo");
    command.arg("/usr/sbin/ufw");
    command.arg("allow");
    command.arg("in");
    command.arg("proto");
    command.arg("udp");
    command.arg("from");
    command.arg(sender_ip.to_string());
    command.arg("to");
    command.arg(listen_address.ip().to_string());
    command.arg("port");
    command.arg(listen_address.port().to_string());
    command.arg("comment");
    command.arg("Bit to Byte direct test");

    match command.status() {
        Ok(status) => {
            if !status.success() {
                return Err(String::from(
                    "Firewall permission was not granted. Check the sudo/UFW output above. No connection test was started.",
                ));
            }
        }
        Err(error) => return Err(format!("Could not request firewall permission: {}", error)),
    }

    println!("UFW rule command completed. Windows/WSL and router permissions are separate.");
    return Ok(());
}

#[cfg(not(target_os = "linux"))]
pub fn request_access(_sender_ip: IpAddr, _listen_address: SocketAddr) -> Result<(), String> {
    return Err(String::from(
        "The permission helper currently supports Ubuntu/UFW only. Run without --allow-from and configure this computer's firewall separately.",
    ));
}
