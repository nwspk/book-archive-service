#[macro_use] extern crate rocket;
use rocket::form::{Contextual, Form};
use rocket::futures::TryFutureExt;
use rocket::log::private::Record;
use rocket::serde;
use rocket_dyn_templates::{Template, context};
use rocket::State;
use core::option::Option;
use rocket::serde::json::Json;

#[derive(Clone)]
pub struct Secrets {
    airtable_api_key: String,
}
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
    last_updated: std::time::Instant
}

#[derive(FromForm)]
struct CheckoutBook<'r> {
    book_data_list: &'r str,
    borrower_data_list: &'r str
}

#[post("/checkout_book_form_submit", data = "<checkout_book>")]
fn checkout_book_form_submit(checkout_book: Form<CheckoutBook<'_>>) {
    println!("Submitted {} by {}",
             checkout_book.book_data_list,
             checkout_book.borrower_data_list
    );

    //note - we should then display like a "thank you for submitting the book!"
    //or "error has occured while submitting the book - please try again later" kind of thing
}

#[get("/checkout_book_form")]
fn checkout_book_form() -> Template {
    Template::render("checking_out_form", context!{
    })
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


#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    let airtable_api_key = std::fs::read_to_string("airtable-token-secret").unwrap();
    rocket::build()
        .mount("/", routes![
            index,
            checkout_book_form_submit,
            checkout_book_form,
            return_book_data
        ])
        .attach(Template::fairing())
        .manage(Secrets {airtable_api_key})
}
