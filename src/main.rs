#[macro_use] extern crate rocket;
use rocket::form::{Contextual, Form};
use rocket::futures::TryFutureExt;
use rocket_dyn_templates::{Template, context};

pub struct Secrets {
    airtable_api_key: String,
}
pub struct ArchiveUser {
    id: String,
    name: String,
}

pub struct Book {
    id: String,
    name: String,
    number_available: u8,
}

struct AirtableResponse

async fn get_all_books(secrets: Secrets) -> Result<Vec<Book>, reqwest::Error> {
    let mut books: Vec<Book> = Vec::new();
    let mut offset: Some<&str> = None;
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


        if offset == None {
            break;
        }
    }
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
    let users = vec![
        "Casimir",
        "Mel",
        "Tristan",
    ];
    let books = vec![
        "Book 1",
        "Book 2",
        "Book 3",
    ];
    Template::render("checking_out_form", context!{
        users: users,
        books: books
    })
}


#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    let airtable_api_key = std::fs::read_to_string("airtable-token-secret").unwrap();
    rocket::build()
        .mount("/", routes![index, checkout_book_form_submit, checkout_book_form])
        .attach(Template::fairing())
        .manage(Secrets {airtable_api_key})
}
