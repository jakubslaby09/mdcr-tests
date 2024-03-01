use std::{error::Error, fs::File, string::String};

use reqwest::blocking::Client;
use scraper::{selectable::Selectable, ElementRef, Html, Selector};
use serde::Serialize;
use std::io::Write;

const URL: &str = "https://etesty2.mdcr.cz/Vestnik/ShowPartial";
const OUTPUT_CSV: &str = "./scrape.csv";
const OUTPUT_HTML: &str = "./scrape.html";
const PAGE_LIMIT: usize = 200;

const QUESTION_SELECTOR: &str = "body > div.QuestionPanel";
const QUESTION_TEXT_SELECTOR: &str = "div.QuestionText";
const QUESTION_CODE_SELECTOR: &str = "span.QuestionCode";
const QUESTION_CHANGE_DATE_SELECTOR: &str = "span.QuestionChangeDate";
const QUESTION_ANSWER_SELECTOR: &str = "div.AnswersPanel > div.Answer";
const QUESTION_ANSWER_TEXT_SELECTOR: &str = "div.AnswerText > a";
const QUESTION_VIDEO_SELECTOR: &str = "div.QuestionImagePanel > video > source";
const QUESTION_IMAGE_SELECTOR: &str = "div.QuestionImagePanel > img";

// TODO: #![deny(clippy::unwrap_used)]

fn main() {
    let client = Client::new();

    let csv_file = File::create(OUTPUT_CSV).expect("couldn't make a file");
    let mut html_file = File::create(OUTPUT_HTML).expect("couldn't make a file");
    let mut writer = csv::Writer::from_writer(csv_file);

    for page in 1..PAGE_LIMIT {
        if let Some(res) = parse_page(&client, page, &mut html_file).unwrap() {
            eprintln!("fetched and parsed page {page}");
            for question in res {
                writer.serialize(question).unwrap();
            }
        } else {
            eprintln!("done scraping {} pages", page.saturating_sub(1));
            break;
        }
    }

    eprintln!("writing to a file...");
    // writeln!(result_file, "{}", ).expect("couldn't write results to a file");
    eprintln!("done");
}

fn parse_page(client: &Client, page: usize, output_html: &mut File) -> Result<Option<Vec<Question>>, Box<dyn Error>> {
    let res = client.post(URL)
    .body(format!("page={page}"))
    .header("content-type", "application/x-www-form-urlencoded")
    .send()?;

    let text = res.text()?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    
    writeln!(output_html, "<!-- Page {page} -->\n{}", text).expect("couldn't write results to a file");
    let document = Html::parse_document(&text);

    let questions_selector = Selector::parse(QUESTION_SELECTOR).unwrap();
    let questions = document.select(&questions_selector);
    
    Ok(Some(questions.map(
        |question| parse_question(question).expect("question error")
    ).collect()))
}

fn parse_question(element: ElementRef) -> Result<Question, String> {
    let question_text_selector = Selector::parse(QUESTION_TEXT_SELECTOR).unwrap();
    let question_text_element = element.select(&question_text_selector).nth(0).unwrap();
    let question_date_selector = Selector::parse(QUESTION_CHANGE_DATE_SELECTOR).unwrap();
    let question_code_selector = Selector::parse(QUESTION_CODE_SELECTOR).unwrap();
    let question_answer_selector = Selector::parse(QUESTION_ANSWER_SELECTOR).unwrap();
    let question_answer_text_selector = Selector::parse(QUESTION_ANSWER_TEXT_SELECTOR).unwrap();
    let question_video_selector = Selector::parse(QUESTION_VIDEO_SELECTOR).unwrap();
    let question_image_selector = Selector::parse(QUESTION_IMAGE_SELECTOR).unwrap();

    let mut answers: Vec<(bool, String)> = element.select(&question_answer_selector).map(|answer_element| 
        (answer_element.attr("data-correct").unwrap() == "True",
        answer_element.select(&question_answer_text_selector).nth(0).unwrap().text().collect::<String>())
    ).collect();
    answers.sort_by_key(|answer| answer.0);

    Ok(Question {
        question: question_text_element.text().last().unwrap().replace('\n', " ").trim().to_string(),
        change_date: question_text_element.select(&question_date_selector).nth(0).ok_or(question_text_element.inner_html())?.text().collect::<String>(),
        code: question_text_element.select(&question_code_selector).nth(0).unwrap().text().collect::<String>()/* .parse() */,
        
        // answers: ["".to_string(), "".to_string(), "".to_string()],
        
        right_answer: answers.iter().nth(0).unwrap().1.replace('\n', " ").trim().to_string(),
        first_wrong_answer: answers.iter().nth(1).map(|it| it.1.replace('\n', " ").trim().to_string()),
        second_wrong_answer: answers.iter().nth(2).map(|it| it.1.replace('\n', " ").trim().to_string()),
        media: element.select(&question_video_selector).nth(0).map(
            |it| QuestionMedia::Video(it.attr("src").unwrap().to_string())
        ).or(element.select(&question_image_selector).nth(0).map(
            |it| QuestionMedia::Image(it.attr("src").unwrap().to_string()
        ))).unwrap_or(QuestionMedia::None),
    })
}

#[derive(Debug, Serialize)]
struct Question {
    code: String,
    change_date: String,
    question: String,
    media: QuestionMedia,
    /// The first one is correct
    // answers: [String; 3],
    right_answer: String,
    first_wrong_answer: Option<String>,
    second_wrong_answer: Option<String>,
}

#[derive(Debug, Serialize)]
enum QuestionMedia {
    Image(String),
    Video(String),
    #[serde(rename = "")]
    None,
}

#[derive(Debug)]
struct NoPagesLeftError {
    pub page: usize
}

impl std::fmt::Display for NoPagesLeftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no pages left at page {}", self.page)
    }
}
impl Error for NoPagesLeftError {}