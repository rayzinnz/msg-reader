use std::{env, path::Path};

use helper_lib::{asyncs::{TxLevel,TxMsg}, llm::{LLMEndpoint, LLMCloudflare}};
use log::*;
use msg_reader::{convert_to_markdown, get_msg_from_file};
use tokio::{runtime::Runtime, sync::mpsc};

//TODO move to config.toml
const URL_BASE:&str = "https://vlm.rayzinnz.com";
const CF_ACCESS_CLIENT_ID:&str = "b90805322b201e6ed655aaddff1effe9.access";
const CF_ACCESS_CLIENT_SECRET_ENV:&str = "CF_ACCESS_CLIENT_SECRET";

fn main() {
    helper_lib::setup_logger(LevelFilter::Info);

    let filepath = Path::new(r"C:\Users\hrag\OutlookData\ITSolutionsTickets\2026\00000000812638D851360E448831BA5A9840D7BF07002E78EC35ADE8584380BC921B7514433600000000770600002E78EC35ADE8584380BC921B7514433600000000BFFF0000.msg");
    let msg = get_msg_from_file(filepath).unwrap();

    // println!("{}", msg.html);
    println!("{}", msg.sender_smtp_address);
    // println!("{:#?}", msg.sub_msgs[0].recipients);
    // println!("{:#?}", msg.sub_msgs[0].attachments[0].display_name);

    // for att in &msg.attachments {
    //     println!("{}", att.content_id)
    // }

    // if let Ok(rt) = Runtime::new() {
    // 	let _rt_result = rt.block_on(async {
    //         let cf_access_client_secret = &env::var(CF_ACCESS_CLIENT_SECRET_ENV).expect(&format!("could not get env var {}", CF_ACCESS_CLIENT_SECRET_ENV));
    //         let llm_endpoint = LLMEndpoint::Cloudflare(LLMCloudflare { 
    //             url: URL_BASE.to_string(), 
    //             access_client_id: CF_ACCESS_CLIENT_ID.to_string(), 
    //             access_client_secret: cf_access_client_secret.to_string()
    //         });
            
    //         let (progress_tx, mut progress_rx) = mpsc::channel::<TxMsg>(32);
            
    //         // Spawn the work task in a separate task so we can receive progress concurrently
    //         let work_handle = tokio::task::spawn_blocking(move || { convert_to_markdown(&msg, true, &Some(llm_endpoint), Some(&progress_tx)) });
    //         //let work_handle = tokio::task::spawn_blocking(move || { convert_to_markdown(&msg, true, &None, Some(&progress_tx)) });
    //         // let md = convert_to_markdown(&msg, true, &None, Some(progress_tx)).await.unwrap();
    //         // println!("{}", md);

    //         // Receive and print progress messages as they arrive
    //         while let Some(txmsg) = progress_rx.recv().await {
    //             match txmsg.txlevel {
    //                 TxLevel::Progress => { println!("{}", txmsg.message); },
    //                 TxLevel::PrintLn => { println!("{}", txmsg.message); },
    //                 TxLevel::Error => { error!("{}", txmsg.message); }
    //                 TxLevel::Warn => { warn!("{}", txmsg.message); }
    //                 TxLevel::Info => { info!("{}", txmsg.message); }
    //                 TxLevel::Debug => { debug!("{}", txmsg.message); }
    //                 TxLevel::Trace => { trace!("{}", txmsg.message); }
    //             }
    //         }

    //         // Wait for the work to finish and get the final result
    //         match work_handle.await.unwrap() {
    //             Ok(md) => (), //println!("[RESULT] {}", md),
    //             Err(e) => eprintln!("[ERROR] {}", e),
    //         }            
    //     });
    // }


}
