use std::{path::Path};

use helper_lib::{asyncs::{TxLevel,TxMsg}};
#[cfg(feature = "markdown")]
use helper_lib::llm::{LLMEndpoint, LLMCloudflare};
use log::*;
use msg_reader::{get_msg_from_file};
#[cfg(feature = "markdown")]
use msg_reader::convert_to_markdown_async;
use tokio::{runtime::Runtime, sync::mpsc};

//TODO move to config.toml
#[cfg(feature = "markdown")]
const URL_BASE:&str = "https://vlm.rayzinnz.com";

fn main() {
    helper_lib::setup_logger(LevelFilter::Debug, None, "", "html5ever");

    // let filepath = Path::new(r"C:\Users\hrag\temp\msg_examples\msg_in_msg.msg");
    let filepath = Path::new(r"C:\Users\hrag\temp\msg_examples\pkcs7_signed_email_p7m.msg");
    let msg = get_msg_from_file(filepath).unwrap();

    // println!("{}", msg.html);
    println!("{}", msg.sender_smtp_address);
    // println!("{:#?}", msg.sub_msgs[0].recipients);
    // println!("{:#?}", msg.sub_msgs[0].attachments[0].display_name);

    // for att in &msg.attachments {
    //     println!("{}", att.content_id)
    // }

    // let runtime = tokio::runtime::Runtime::new().expect("Error starting tokio::runtime");
    // thread::scope(|s| { runtime.block_on( async { 

    //     let (progress_tx, mut progress_rx) = mpsc::channel::<TxMsg>(32);
    //     let join_handle = thread::Builder::new()
    //         .spawn_scoped(s, move || { convert_to_markdown(&msg, true, &None, Some(&progress_tx), &mut vec![]) })
    //         .expect("Failed to spawn thread");
    //     while let Some(txmsg) = progress_rx.recv().await {
    //         match txmsg.txlevel {
    //             TxLevel::Progress => { println!("{}", txmsg.message); },
    //             TxLevel::PrintLn => { println!("{}", txmsg.message); },
    //             TxLevel::Error => { error!("{}", txmsg.message); }
    //             TxLevel::Warn => { warn!("{}", txmsg.message); }
    //             TxLevel::Info => { info!("{}", txmsg.message); }
    //             TxLevel::Debug => { debug!("{}", txmsg.message); }
    //             TxLevel::Trace => { trace!("{}", txmsg.message); }
    //         }
    //     }
    //     match join_handle.join().unwrap() {
    //         Ok(md) => {

    //         },
    //         Err(e) => {
    //             eprintln!("[ERROR] {}", e);
    //         }
    //     }

    // });});

    if let Ok(rt) = Runtime::new() {
    	let _rt_result = rt.block_on(async {
            #[cfg(feature = "markdown")]
            let llm_endpoint = LLMEndpoint::Cloudflare(LLMCloudflare { 
                url: URL_BASE.to_string(), 
                access_client_id: None, 
                access_client_secret: None
            });
            
            let (progress_tx, mut progress_rx) = mpsc::channel::<TxMsg>(32);

            //example pre-described file
            // let fd_example = FileDescription { file_crc: 3741471923040398142, description: String::from("this is this files description") };
            // let fd_example2 = FileDescription { file_crc: 5887996252687428101, description: String::from("this is another files description") };
            
            // Spawn the work task in a separate task so we can receive progress concurrently
            // let work_handle = tokio::task::spawn_blocking(move || { convert_to_markdown(&msg, true, &Some(llm_endpoint), Some(&progress_tx), &mut vec![]) });
            //let work_handle = tokio::task::spawn_blocking(move || { convert_to_markdown(&msg, true, &None, Some(&progress_tx), &mut vec![]) });
            #[cfg(feature = "markdown")]
            let work_handle = tokio::task::spawn(async move { convert_to_markdown_async(&msg, true, &None, Some(&progress_tx), &mut vec![]).await });
            // let md = convert_to_markdown(&msg, true, &None, Some(progress_tx)).await.unwrap();
            // println!("{}", md);

            // Receive and print progress messages as they arrive
            while let Some(txmsg) = progress_rx.recv().await {
                match txmsg.txlevel {
                    TxLevel::Progress => { println!("{}", txmsg.message); },
                    TxLevel::PrintLn => { println!("{}", txmsg.message); },
                    TxLevel::Error => { error!("{}", txmsg.message); panic!("test panic") }
                    TxLevel::Warn => { warn!("{}", txmsg.message); }
                    TxLevel::Info => { info!("{}", txmsg.message); }
                    TxLevel::Debug => { debug!("{}", txmsg.message); }
                    TxLevel::Trace => { trace!("{}", txmsg.message); }
                }
            }

            // Wait for the work to finish and get the final result
            #[cfg(feature = "markdown")]
            match work_handle.await.unwrap() {
                Ok(md) =>
                    // (),
                    println!("[RESULT] {}", md),
                Err(e) => eprintln!("[ERROR] {}", e),
            }            
        });
    }


}
