use std::path::Path;

use msg_reader::{get_msg_from_file};

fn main() {
    let filepath = Path::new(r"C:\Users\hrag\OutlookData\ITSolutionsTickets\2026\00000000812638D851360E448831BA5A9840D7BF07002E78EC35ADE8584380BC921B7514433600000000770300002E78EC35ADE8584380BC921B7514433600015D8912CE0000.msg");
    let msg = get_msg_from_file(filepath).unwrap();

    println!("{:#?}", msg.recipients);
}
