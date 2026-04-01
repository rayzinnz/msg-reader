//ref: https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxmsg/b046868c-9fbf-41ae-9ffb-8de2bd4eec82
//     https://officeprotocoldoc.z19.web.core.windows.net/files/MS-OXMSG/%5BMS-OXMSG%5D-080425.pdf

use std::{fs::File, io::Read, path::{Path, PathBuf}};

use anyhow::{Result, bail};
use cfb::CompoundFile;
use chrono::{DateTime, TimeZone, Utc};
use compressed_rtf::decompress_rtf;
use encoding_rs::UTF_16LE;

mod rtf_html_deencapsulate;

#[derive(Debug)]
#[allow(dead_code)]
struct MsgProperty {
	property_type: u16,
	property_id: u16,
	property_flags: u32,
	value: u64,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MsgAttachment {
	pub display_name: String, //3001
	pub filename: String, //3704
	pub long_filename: String, //3707
	pub content_id: String, //3712
	pub mimetype: String, //370E
	pub data: Vec<u8>, //3701
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MsgRecipient {
	pub display_name: String, //3001
	pub email_address: String, //3003
	pub smtp_address: String, //39FE
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MsgContents {
	pub sent_time: Option<DateTime<Utc>>, //0039 (from __properties_version1.0)
	pub received_time: DateTime<Utc>, //0E06 (from __properties_version1.0)
	pub subject: String, //0037
	pub normalized_subject: String, //0E1D
	pub conversation_topic: String, //0070
	pub conversation_index: Vec<u8>, //0071
	pub sender_name: String, //0C1A
	pub sender_email_address: String, //0C1F
	pub sender_smtp_address: String, //5D01
	pub received_by_name: String, //0040
	pub received_by_email_address: String, //0076
	pub received_by_smtp_address: String, //5D07
	pub display_bcc: String, //0E02
	pub display_cc: String, //0E03
	pub display_to: String, //0E04
	pub transport_message_headers: String, //007D
	pub text: String, //1000
	pub html: String, //1013 (html), 1009 (rtf)
	pub attachments: Vec<MsgAttachment>,
	pub sub_msgs: Vec<MsgContents>,
	pub recipients: Vec<MsgRecipient>,
}

pub fn windows_filetime_to_utc(windows_filetime:u64) -> DateTime<Utc> {
	//windows filetime = 100-nanosecond intervals since January 1, 1601 UTC
	let us_since_1601 = windows_filetime / 10; // Convert 100-nanosecond intervals to microseconds
	let td = chrono::Duration::microseconds(us_since_1601 as i64); // Convert to Duration and add to 1601-01-01 00:00:00 UTC
	let utc_date = Utc.with_ymd_and_hms(1601,1,1,0,0,0).unwrap();
	let utc_date = utc_date.checked_add_signed(td).unwrap();
	utc_date
}

fn get_substg_string(cfbf: &mut CompoundFile<File>, path: &Path, tag: &str) -> String {
	let mut rtn = String::new();

	//0x001F UTF_16LE, 0x001E ASCII
	let tagdir_utf16le = format!("__substg1.0_{tag}001F");
	let tagdir_ascii = format!("__substg1.0_{tag}001E");

	if cfbf.exists(path.join(&tagdir_utf16le)) {
		if let Ok(mut stream) = cfbf.open_stream(path.join(&tagdir_utf16le)) {
			let mut data = Vec::new();
			if let Ok(_) = stream.read_to_end(&mut data) {
				let data = UTF_16LE.decode(&data);
				rtn = data.0.to_string();
			}
		}
	} else if cfbf.exists(path.join(&tagdir_ascii)) {
		if let Ok(mut stream) = cfbf.open_stream(path.join(&tagdir_ascii)) {
			let mut data = Vec::new();
			if let Ok(_) = stream.read_to_end(&mut data) {
				let data = String::from_utf8_lossy(&data);
				rtn = data.to_string();
			}
		}
	}

	return rtn
}

fn get_substg_binary(cfbf: &mut CompoundFile<File>, path: &Path, tag: &str) -> Vec<u8> {
	//0x0102 Binary
	let tagdir_binary = format!("__substg1.0_{tag}0102");

	if cfbf.exists(path.join(&tagdir_binary)) {
		if let Ok(mut stream) = cfbf.open_stream(path.join(&tagdir_binary)) {
			let mut data = Vec::new();
			if let Ok(_) = stream.read_to_end(&mut data) {
				return data
			}
		}
	}

	return Vec::new()
}

fn get_msg_properties(cfbf: &mut CompoundFile<File>) -> Result<Vec<MsgProperty>> {
	let mut msg_properties = Vec::new();
	let tagdir = "/__properties_version1.0";
	if cfbf.exists(&tagdir) {
		if let Ok(mut stream) = cfbf.open_stream(&tagdir) {
			let mut data = Vec::new();
			if let Ok(_) = stream.read_to_end(&mut data) {
				//data should have 32 byte header, then lot of 16 byte properties
				if data.len() % 16 != 0 {
					bail!("Property stream not divisible by 16");
				}
				if data.len() <= 32 {
					return Ok(msg_properties);
				}
				// Skip 32-byte header, then loop through 16-byte property entries
				for chunk in data[32..].chunks(16) {
					let property_data: [u8; 16] = chunk.try_into()?;
					let msg_property = MsgProperty {
						property_type: u16::from_le_bytes(property_data[0..2].try_into()?),
						property_id: u16::from_le_bytes(property_data[2..4].try_into()?),
						property_flags: u32::from_le_bytes(property_data[4..8].try_into()?),
						value: u64::from_le_bytes(property_data[8..16].try_into()?),
					};
					msg_properties.push(msg_property);
				}
			}
		}
	} else {
		bail!("'{}' does not exist", tagdir);
	}

	Ok(msg_properties)
}

pub fn get_msg(cfbf: &mut CompoundFile<File>, path: PathBuf) -> Result<MsgContents> {
	let mut html;

	//data stored in msg properties
	//06 0E: 0x0E06, MessageDeliveryTime (AKA Received Time)
	let message_delivery_time_id:u16 = u16::from_le_bytes([06, 14]);
	let received_time: DateTime<Utc>;
	//39 00: 0x0039, ClientSubmitTime (AKA Sent Time)
	let client_submit_time_id:u16 = u16::from_le_bytes([57, 00]);
	let sent_time: Option<DateTime<Utc>>;
	let msg_properties = get_msg_properties(cfbf)?;
	match msg_properties.iter().find(|p| p.property_id==message_delivery_time_id) {
		Some(message_delivery_time) => received_time = windows_filetime_to_utc(message_delivery_time.value),
		None => bail!("Could not find the message received time (message_delivery_time)")
	}
	match msg_properties.iter().find(|p| p.property_id==client_submit_time_id) {
		Some(client_submit_time) => sent_time = Some(windows_filetime_to_utc(client_submit_time.value)),
		None => sent_time = None
	}

	//subject 0x0037 Subject
	let subject= get_substg_string(cfbf, &path, "0037");

	//body 0x1013 HTML
	html = get_substg_string(cfbf, &path, "1013");

	//body 0x1009, RtfCompressed, 0x0102 Binary
	//LZFu compression (MELA = uncompressed)
	//compressed-rtf implements Microsoft’s MS‑OXRTFCP compression/decompression.
	if html.is_empty() {
		let data = get_substg_binary(cfbf, &path, "1009");
		let rtf = decompress_rtf(&data)?;
		// println!("{}", rtf);
		//De‑encapsulate HTML
		html = rtf_html_deencapsulate::rtf_to_html_outlook(&rtf).unwrap_or_default();
		// println!("{}", html);
	}

	//body 0x1000 Body
	let text = get_substg_string(cfbf, &path, "1000");
	// println!("{}", text);
	if html.is_empty() {
		html = text.clone();
	}

	//attachments
	let mut sub_paths: Vec<PathBuf> = Vec::new();
	// let mut recipient_paths: Vec<PathBuf> = Vec::new();
	if let Ok(entries) = cfbf.read_storage(&path) {
		for entry in entries {
			if entry.is_storage() {
				// println!("{:#?}", entry);
				// if entry.name().starts_with("__attach_") {
					// println!("{:?}", entry.path());
					let sub_path = entry.path().to_path_buf();
					sub_paths.push(sub_path);
				// } else if entry.name().starts_with("__attach_") {
				// 	let sub_path = entry.path().to_path_buf();
				// 	recipient_paths.push(sub_path);
				// }
			}
		}
	}

	let mut attachments: Vec<MsgAttachment> = Vec::new();
	let mut sub_msgs: Vec<MsgContents> = Vec::new();
	let mut recipients: Vec<MsgRecipient> = Vec::new();
	for sub_path in sub_paths {
		// println!("{:?}", sub_path.iter().last());
		let last_path_component = sub_path.iter().last().unwrap_or_default().to_string_lossy();
		if last_path_component.starts_with("__attach_") {
			//is file or msgobj?
			//attachment binary, 0x3701 AttachDataObject, 0x0102 PT_BINARY
			let is_file = cfbf.exists(&sub_path.join("__substg1.0_37010102"));
			//attachment msg path, 0x3701 AttachDataObject, 0x000D PT_OBJECT
			let is_msgobj = cfbf.exists(&sub_path.join("__substg1.0_3701000D"));
			if !(is_file || is_msgobj) {
				bail!("msg attachment is not a file or a msg")
			}
			if is_file && is_msgobj {
				bail!("msg attachment is both a file and a msg")
			}
			if is_file {
				let display_name = get_substg_string(cfbf, &sub_path, "3001");
				let filename = get_substg_string(cfbf, &sub_path, "3074");
				let long_filename = get_substg_string(cfbf, &sub_path, "3707");
				let content_id = get_substg_string(cfbf, &sub_path, "3712");
				let mimetype = get_substg_string(cfbf, &sub_path, "370E");
				let data = get_substg_binary(cfbf, &sub_path, "3701");

				let att:MsgAttachment = MsgAttachment {
					display_name: display_name,
					filename: filename,
					long_filename: long_filename,
					content_id: content_id,
					mimetype: mimetype,
					data: data,
				};
				attachments.push(att);
			}
			if is_msgobj {
				// println!("{}", sub_path.to_string_lossy());
				let msg_contents = get_msg(cfbf, sub_path.join("__substg1.0_3701000D"))?;
				//println!("{:#?}", msg_contents.html);
				sub_msgs.push(msg_contents);
			}
		} else if last_path_component.starts_with("__recip_") {
			let recipient:MsgRecipient = MsgRecipient {
				display_name: get_substg_string(cfbf, &sub_path, "3001"),
				email_address: get_substg_string(cfbf, &sub_path, "3003"),
				smtp_address: get_substg_string(cfbf, &sub_path, "39FE")
			};
			recipients.push(recipient);
		}
	}

	let rtn:MsgContents = MsgContents {
		sent_time,
		received_time,
		subject,
		normalized_subject: get_substg_string(cfbf, &path, "0E1D"),
		conversation_topic: get_substg_string(cfbf, &path, "0070"), //0070
		conversation_index: get_substg_binary(cfbf, &path, "0071"), //
		sender_name: get_substg_string(cfbf, &path, "0C1A"), //0C1A
		sender_email_address: get_substg_string(cfbf, &path, "0C1F"), //0C1F
		sender_smtp_address: get_substg_string(cfbf, &path, "5D01"), //5D01
		received_by_name: get_substg_string(cfbf, &path, "0040"), //0040
		received_by_email_address: get_substg_string(cfbf, &path, "0076"), //0076
		received_by_smtp_address: get_substg_string(cfbf, &path, "5D07"), //5D07
		display_bcc: get_substg_string(cfbf, &path, "0E02"), //0E02
		display_cc: get_substg_string(cfbf, &path, "0E03"), //0E03
		display_to: get_substg_string(cfbf, &path, "0E04"), //0E04
		transport_message_headers: get_substg_string(cfbf, &path, "007D"), //007D
		text,
		html,
		attachments,
		sub_msgs,
		recipients,
	};

	return Ok(rtn)
}

pub fn get_msg_from_file(msg_filepath:&Path) -> Result<MsgContents> {
	let mut cfbf = cfb::open(msg_filepath)?;
	get_msg(&mut cfbf, PathBuf::from("/"))
}