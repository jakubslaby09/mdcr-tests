use std::path::PathBuf;
use std::{fs::File, string::String};

use anyhow::Result;
use reqwest::{blocking::Client, Url};
use scraper::{selectable::Selectable, ElementRef, Html, Selector};
use serde::Serialize;
use std::io::Write;
use std::cmp::Reverse;

const SECTIONS_URL: &str = "https://etesty2.mdcr.cz/Vestnik";
const SECTION_BASE_URL: &str = "https://etesty2.mdcr.cz";
const ALL_URL: &str = "https://etesty2.mdcr.cz/Vestnik/ShowPartial";
const OUTPUT_CSV: &str = "./scrape.csv";
const OUTPUT_HTML: &str = "./scrape.html";
const PAGE_LIMIT: usize = 200;

const SECTIONS_SELECTOR: &str = "#VerticalMenuPanel > ul > li > a";
const QUESTION_SELECTOR: &str = "body > div.QuestionPanel";
const QUESTION_TEXT_SELECTOR: &str = "div.QuestionText";
const QUESTION_CODE_SELECTOR: &str = "span.QuestionCode";
const QUESTION_CHANGE_DATE_SELECTOR: &str = "span.QuestionChangeDate";
const QUESTION_ANSWER_SELECTOR: &str = "div.AnswersPanel > div.Answer";
const QUESTION_ANSWER_TEXT_SELECTOR: &str = "div.AnswerText > a";
const QUESTION_VIDEO_SELECTOR: &str = "div.QuestionImagePanel > video > source";
const QUESTION_IMAGE_SELECTOR: &str = "div.QuestionImagePanel > img";

fn out_html_path(section: &Section) -> PathBuf {
    format!("./scrape.{}.{}.html", section.section_id, truncate(&section.name, 30)).into()
}
fn out_csv_path(section: &Section) -> PathBuf {
    format!("./scrape.{}.{}.csv", section.section_id, truncate(&section.name, 30)).into()
}

// TODO: #![deny(clippy::unwrap_used)]

fn main() -> Result<()> {
    let client = Client::new();
    
    eprintln!("scraping full");
    scrape_section(&client, None)?;
    eprintln!(" listing sections");
    for section in list_sections(&client)?.into_iter() {
        eprintln!(" scraping {}", section.name);
        scrape_section(&client, Some(section))?
    };
    eprintln!("done");
    Ok(())
}

fn list_sections(client: &Client) -> Result<Vec<Section>> {
    let res = client.get(SECTIONS_URL).send()?.text()?;
    let selector = Selector::parse(SECTIONS_SELECTOR).unwrap();
    let html = Html::parse_document(&res);
    Ok(html.select(&selector).map(|link| {
        let href = link.attr("href").unwrap();
        Section {
            name: link.text().collect(),
            section_id: Url::parse(&format!("{SECTION_BASE_URL}{href}")).unwrap()
            .query_pairs().find(|q| q.0 == "basketScope").unwrap().1.to_string(),
        }
    }).collect())
}

fn scrape_section(client: &Client, section: Option<Section>) -> Result<()> {
    let output_html_path = match &section {
        Some(section) => out_html_path(&section),
        None => OUTPUT_HTML.into(),
    };
    let output_csv_path = match &section {
        Some(section) => out_csv_path(&section),
        None => OUTPUT_CSV.into(),
    };
    let mut output_html = File::create(output_html_path).expect("couldn't make a file");
    let csv_file = File::create(output_csv_path).expect("couldn't make a file");
    let mut csv_writer = csv::Writer::from_writer(csv_file);
    
    for page in 1..PAGE_LIMIT {
        if let Some(res) = scrape_page(&client, section.as_ref(), page, &mut output_html).unwrap() {
            eprintln!("  fetched and parsed page {page}");
            for question in res {
                csv_writer.serialize(question).unwrap();
            }
        } else {
            eprintln!("  done scraping {} pages", page.saturating_sub(1));
            break;
        }
    }

    Ok(())
}

fn scrape_page(client: &Client, section: Option<&Section>, page: usize, output_html: &mut File) -> Result<Option<Vec<Question>>> {
    // let url = Url::parse_with_params(match section {
    //     Some(section) => section.url.as_str(),
    //     None => ALL_URL,
    // }, &[("page", &page.to_string())])?;
    let url = match section {
        Some(section) => Url::parse_with_params(ALL_URL, &[("page", &page.to_string()), ("basketScope", &section.section_id)]),
        None => Url::parse_with_params(ALL_URL, &[("page", &page.to_string()), ]),
    }?;
    let res = client.get(url)
    .send()?.text()?;

    if res.trim().is_empty() {
        return Ok(None);
    }
    
    writeln!(output_html, "<!-- Page {page} -->\n{}", res).expect("couldn't write results to a file");
    let document = Html::parse_document(&res);

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
    answers.sort_by_key(|answer| Reverse(answer.0));

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

fn truncate(name: &str, max: usize) -> String {
    if name.chars().count() < max {
        return name.to_string();
    }
    name.chars().take(max).collect()
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
struct Section {
    pub name: String,
    pub section_id: String,
}