#[macro_use] extern crate rocket;
use rocket::form::{Contextual, Form};
use rocket::futures::TryFutureExt;
use rocket::log::private::Record;
use rocket::serde;
use rocket_dyn_templates::{Template, context};
use rocket::State;
use core::option::Option;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicI32, AtomicI64, AtomicU32};
use std::sync::{Mutex, RwLock};
use chrono::{Duration, TimeDelta};
use rocket::http::ContentType;
use rocket::serde::json::Json;
use serde_json::json;
use chrono::prelude::*;
use std::sync::atomic::Ordering::{Relaxed as ordering, Relaxed};


#[derive(Clone)]
pub struct Secrets {
    airtable_api_key: String,
}

#[derive(serde::Serialize)]
pub struct ArchiveUser {
    id: String,
    name: String,
}

#[derive(serde::Serialize, Clone)]
pub struct Book {
    id: String,
    name: String,
    authors: String,
    number_available: u64,
    number_in_stock: u64
}

pub struct BookCache {
    pub books: RwLock<HashMap<String, Book>>,
    pub date: AtomicI64,
}
impl BookCache {
    pub fn new() -> BookCache {
        BookCache {
            books: RwLock::new(HashMap::new()),
            date: AtomicI64::new(0),
        }
    }

    pub async fn refresh_book_data(&self, secrets: &State<Secrets>) {
        let books = get_all_books(secrets).await.unwrap();
        let mut book_map = self.books.write().unwrap();
        book_map.clear();
        for book in books {
            let book_clone = book.clone();
            book_map.insert(book.id, book_clone);
        }
        self.date.store(Utc::now().timestamp_micros(), ordering);
    }

    pub async fn get_books(&self, secrets: &State<Secrets>) -> HashMap<String, Book> {
        let cache_date =
            DateTime::from_timestamp_micros(self.date.load(ordering)).unwrap();
        if (cache_date + TimeDelta::days(1)) < Utc::now() {
            self.refresh_book_data(secrets).await;
        }
        self.books.read().unwrap().clone()
    }

    pub async fn get_book(&self, book_id: String, secrets: &State<Secrets>) -> Option<Book> {
        let cache_date =
            DateTime::from_timestamp_micros(self.date.load(ordering)).unwrap();
        if (cache_date + TimeDelta::days(1)) < Utc::now() {
            self.refresh_book_data(secrets).await;
        }
        self.books.read().unwrap().get(&book_id).cloned()
    }

    pub fn update_cached_number_available(&self, book_id: String, number_available: u64){
        let mut map =  self.books.write().unwrap();
        let mut book = map.get_mut(&book_id).unwrap();
        book.number_available = number_available;
    }


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

#[derive(FromForm)]
struct CheckoutBook<'r> {
    book_data_list: &'r str,
    borrower_data_list: &'r str
}

#[post("/checkout_book_form_submit", data = "<checkout_book>")]
async fn checkout_book_form_submit(
    checkout_book: Form<CheckoutBook<'_>>,
    secrets: &State<Secrets>,
    cache: &State<BookCache>
) -> Template {
    let book = cache.get_book(checkout_book.book_data_list.to_string(), secrets).await.unwrap();

    println!("Checked out {} ({}) by {}",
             checkout_book.book_data_list,
             book.name,
             checkout_book.borrower_data_list
    );
    add_access_log_entry(
        checkout_book.borrower_data_list.to_string(),
        checkout_book.book_data_list.to_string(),
        Utc::now(),
        false,
        secrets
    ).await;
    update_available_number_of_books(book.id, book.number_available-1, secrets, cache).await;

    Template::render("form_submission_response", context!{
        checking_out: true,
        book: book.name,
    })
}

#[post("/return_book_form_submit", data = "<checkout_book>")]
async fn return_book_form_submit(
    checkout_book: Form<CheckoutBook<'_>>,
    secrets: &State<Secrets>,
    cache: &State<BookCache>
)  -> Template {
    let book = cache.get_book(checkout_book.book_data_list.to_string(), secrets).await.unwrap();

    println!("Returned {} ({}) by {}",
             checkout_book.book_data_list,
             book.name,
             checkout_book.borrower_data_list
    );
    add_access_log_entry(
        checkout_book.borrower_data_list.to_string(),
        checkout_book.book_data_list.to_string(),
        Utc::now(),
        true,
        secrets
    ).await;

    update_available_number_of_books(book.id, book.number_available+1, secrets, cache).await;

    Template::render("form_submission_response", context!{
        checking_out: false,
        book: book.name,
    })
}

#[get("/checkout_book_form")]
fn checkout_book_form() -> Template {
    Template::render("checking_out_form", context!{
    })
}

#[get("/return_book_form")]
fn return_book_form() -> Template {
    Template::render("returning_form", context!{
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
async fn return_user_data(secrets: &State<Secrets>) -> Json<Vec<ArchiveUser>> {
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
                number_in_stock: record["fields"]["Copies in stock"].as_number().unwrap().as_u64().unwrap(),
                authors: record["fields"]["Authors"].as_str().unwrap_or("").to_string(),
            };

            books.push(book);

        }

        if offset == None {
            break;
        }
    }
    Ok(books)
}

#[get("/return_available_book_data")]
async fn return_available_book_data(secrets: &State<Secrets>, cache: &State<BookCache>) -> Json<Vec<Book>> {
    let all_books = cache.get_books(secrets).await.into_values();

    let available_books = all_books
        .filter(|book| book.number_available > 0)
        .collect();

    Json(available_books)
}

#[get("/return_taken_out_book_data")]
async fn return_taken_out_book_data(secrets: &State<Secrets>,cache: &State<BookCache>) -> Json<Vec<Book>> {
    let all_books = cache.get_books(secrets).await.into_values();
    let taken_out_books = all_books
        .filter(|book| book.number_available < book.number_in_stock)
        .collect();

    Json(taken_out_books)
}

async fn update_available_number_of_books(
    book_id: String,
    new_number: u64,
    secrets: &State<Secrets>,
    cache: &State<BookCache>
) {
    let client = reqwest::Client::new();

    let json_body = json!({
        "records": [{
            "id": book_id,
            "fields": {
                "Copies available": new_number,
            }
        }]
    });
    client.patch("https://api.airtable.com/v0/appz1OhNtkhOphFqu/Book%20data")
        .bearer_auth(secrets.airtable_api_key.clone())
        .json(&json_body)
        .send().await.unwrap();
    cache.update_cached_number_available(book_id, new_number);

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

#[launch]
fn rocket() -> _ {
    let airtable_api_key = std::env::var("AIRTABLE-TOKEN").expect("AIRTABLE-TOKEN is not set");
    rocket::build()
        .mount("/", routes![
            checkout_book_form_submit,
            return_book_form_submit,
            checkout_book_form,
            return_book_form,
            return_available_book_data,
            return_taken_out_book_data,
            return_user_data
        ])
        .attach(Template::fairing())
        .manage(Secrets {airtable_api_key})
        .manage(BookCache::new())
}
