#[macro_use] extern crate rocket;
use rocket::form::{Contextual, Form};
use rocket::futures::TryFutureExt;
use rocket::log::private::Record;
use rocket::serde;
use rocket_dyn_templates::{Template, context};
use rocket::State;
use core::option::Option;
use std::fmt::Debug;
use std::sync::Mutex;
use chrono::Duration;
use rocket::http::ContentType;
use rocket::serde::json::Json;
use serde_json::json;
use chrono::prelude::*;

#[derive(Clone)]
pub struct Secrets {
    airtable_api_key: String,
}

#[derive(serde::Serialize)]
pub struct ArchiveUser {
    id: String,
    name: String,
}

#[derive(serde::Serialize)]
pub struct Book {
    id: String,
    name: String,
    authors: String,
    number_available: u64,
}


#[derive(serde::Deserialize)]
pub struct AirtableResponse {
    records: Vec<serde_json::Value>,
    offset: Option<String>
}



pub struct AccessEntry {
    book: Book,
    user_borrowing: ArchiveUser,
}

//might be a good idea to limit the amount of API calls (especially since getting the full list
// of all books takes several API calls)
pub struct BookInMemoryCache {
    users: Vec<ArchiveUser>,
    books: Vec<Book>,
    last_updated: DateTime<Utc>
}

#[derive(FromForm)]
struct CheckoutBook<'r> {
    book_data_list: &'r str,
    borrower_data_list: &'r str
}

#[post("/checkout_book_form_submit", data = "<checkout_book>")]
async fn checkout_book_form_submit(
    checkout_book: Form<CheckoutBook<'_>>,
    secrets: &State<Secrets>
) {
    println!("Submitted {} by {}",
             checkout_book.book_data_list,
             checkout_book.borrower_data_list
    );
    add_access_log_entry(
        checkout_book.borrower_data_list.to_string(),
        checkout_book.book_data_list.to_string(),
        Utc::now(),
        false,
        secrets
    ).await;

    //todo: add proper error handling & response template
    //note - we should then display like a "thank you for submitting the book!"
    //or "error has occured while submitting the book - please try again later" kind of thing
}

#[get("/checkout_book_form")]
fn checkout_book_form() -> Template {
    Template::render("checking_out_form", context!{
    })
}

async fn get_all_users(secrets: &State<Secrets>) -> Result<Vec<ArchiveUser>, reqwest::Error> {
    let mut users: Vec<ArchiveUser> = Vec::new();
    let client = reqwest::Client::new();

    let res = client.get("https://api.airtable.com/v0/appz1OhNtkhOphFqu/tblVrZdqXrmVdpsdD")
        .bearer_auth(secrets.airtable_api_key.clone()).send().await?;
    let response_json = res.json::<AirtableResponse>().await?;
    for record in response_json.records {
        let user = ArchiveUser {
            id: record["id"].as_str().unwrap().to_string(),
            name: record["fields"]["Name"].as_str().unwrap_or("").to_string(),
        };
        users.push(user);
    }
    Ok(users)
}
#[get("/return_user_data")]
async fn return_user_data(secrets: &State<Secrets>, cache: &State<Mutex<Option<BookInMemoryCache>>>) -> Json<Vec<ArchiveUser>> {
    /*if let Some(cache_object) = cache {
        if cache_object.last_updated + Duration::days(1) < Utc::now() {

        }
    }*/
    //todo: implement caching to reduce API calls
    Json(get_all_users(secrets).await.unwrap())
}


async fn get_all_books(secrets: &State<Secrets>) -> Result<Vec<Book>, reqwest::Error> {
    let mut books: Vec<Book> = Vec::new();
    let mut offset: Option<String> = None;
    let client = reqwest::Client::new();

    loop {
        let res = if let Some(offset_token) = offset {
            client.get("https://api.airtable.com/v0/appz1OhNtkhOphFqu/tbl3bXZiWVgZrF81C")
                .bearer_auth(secrets.airtable_api_key.clone())
                .query(&[("offset", offset_token)])
        } else {
            client.get("https://api.airtable.com/v0/appz1OhNtkhOphFqu/tbl3bXZiWVgZrF81C")
                .bearer_auth(secrets.airtable_api_key.clone())
        }.send().await?;
        let response_json = res.json::<AirtableResponse>().await?;
        offset = response_json.offset;

        for record in response_json.records {
            let book = Book{
                id: record["id"].as_str().unwrap().to_string(),
                name: record["fields"]["Title"].as_str().unwrap().to_string(),
                number_available: record["fields"]["Copies available"].as_number().unwrap().as_u64().unwrap(),
                authors: record["fields"]["Authors"].as_str().unwrap_or("").to_string(),
            };

            if book.number_available > 0 {
                books.push(book);
            }
        }

        if offset == None {
            break;
        }
    }
    Ok(books)
}

#[get("/return_book_data")]
async fn return_book_data(secrets: &State<Secrets>) -> Json<Vec<Book>> {
    //todo: implement caching to reduce API calls
    Json(get_all_books(secrets).await.unwrap())
}

async fn add_access_log_entry(
    user_id: String,
    book_id: String,
    date: DateTime<Utc>,
    returning: bool,
    secrets: &State<Secrets>
) {
    let client = reqwest::Client::new();

    let json_body = json!({
            "records": [{
                "fields": {
                    "Date": date.to_string(),
                    "Book": [
                        book_id
                    ],
                    "Checking out/returning?": if returning {"Returning"} else {"Checking out"},
                    "Person borrowing": [
                        user_id
                    ]
                }
            }]
        });

     client.post("https://api.airtable.com/v0/appz1OhNtkhOphFqu/Access%20log")
        .bearer_auth(secrets.airtable_api_key.clone())
        .json(&json_body)
        .send().await.unwrap();

}


#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    let airtable_api_key = std::fs::read_to_string("airtable-token-secret").unwrap();
    let in_memory_cache : Mutex<Option<BookInMemoryCache>> = Mutex::new(None);
    rocket::build()
        .mount("/", routes![
            index,
            checkout_book_form_submit,
            checkout_book_form,
            return_book_data,
            return_user_data
        ])
        .attach(Template::fairing())
        .manage(Secrets {airtable_api_key})
        .manage(in_memory_cache)
}
