use std::path::Path;

use msg_reader::{get_msg_from_file};

fn main() {
    let filepath = Path::new(r"C:\Users\hrag\temp\msg_examples\msg_in_msg.msg");
    let msg = get_msg_from_file(filepath).unwrap();

    println!("{:#?}", msg.recipients);
    println!("{:#?}", msg.sub_msgs[0].recipients);
    println!("{:#?}", msg.sub_msgs[0].attachments[0].display_name);
}
