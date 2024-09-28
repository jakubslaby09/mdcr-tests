import puppeteer, { ProtocolError } from "puppeteer";
import { mkdir, writeFile } from "fs/promises";
import { stringify } from "csv-stringify/sync";
import { stdout } from "process";

const browser = await puppeteer.launch({
    headless: true,
});

const window = await browser.newPage();
/** @type {{[key: string]: string}} */
const assets = {};
const baseUrl = "https://etesty2.mdcr.cz";
/** @const @type {[string, string, string]} */
const answerPrefixes = ["A: ", "B: ", "C: "];
window.on('response', async res => {
    if(res.request().method().toLowerCase() != "get") {
        return;
    }
    const url = res.url();
    const path = new URL(url, baseUrl).pathname;
    if (!path.startsWith('/Content/ImageQuestion/')) {
        return;
    }
    stdout.write(`downloading asset ${path} \x1B[0K`);
    const mime = res.headers()['content-type'];
    /** @type { string } */
    let extension;
    switch (mime) {
        case undefined:
            console.error("missing content-type");
            extension = "";
            break;
        case "video/mp4":
        case "image/gif":
        case "image/jpg":
        case "image/png":
            extension = mime.split("/")[1] ?? (()=>{throw ""})();
            break;
        default:
            console.error(`unknown content-type: ${mime}`);
            extension = mime.replaceAll("/", ".");
    }
    try {
        var buf = await res.buffer();
    } catch (error) {
        if(error instanceof ProtocolError) {
            console.error(`couldn't get body: ${error.name}:\n    ${error.message}`);
        } else {
            console.error(error);
        }
        return;
    }
    assets[url] = `./assets/${path.slice("/Content/ImageQuestion/".length).replaceAll("/", "-")}.${extension}`;
    await mkdir(`./assets`, { recursive: true });
    await writeFile(assets[url], buf);
});

await window.goto(`${baseUrl}/Home/Tests/ro`);
const sections = await window.$$eval(
    "#VerticalMenuPanel > ul:first-of-type > li > a",
    anchors => anchors.map(anchor => /** @type {Section} */ ({
        id: anchor.href.split("/").slice(-1)[0],
        name: anchor.text,
    })
));

for (const section of sections) {
    console.log(`scraping section ${section.name}`);
    /** @type {QuestionsRes} */
    const questionResponses = await (await fetch("https://etesty2.mdcr.cz/Test/GeneratePractise/", {
        method: "POST",
        body: `lectureID=${section.id}`,
        headers: {
            "content-type": "application/x-www-form-urlencoded",
        },
    })).json();

    /** @type {Question[]} */
    const questions = [];
    for (let i = 0; i < questionResponses.Questions.length; i++) {
        const res = /** @type {typeof questionResponses.Questions[0]} */ (questionResponses.Questions[i]);
        stdout.write(`\r  ${i + 1}/${questionResponses.Questions.length}: ${res.Code} \x1B[0K`);
        
        await window.$eval("html", (d, res) => {
            d.innerHTML = `<form action="https://etesty2.mdcr.cz/Test/RenderQuestion" method="POST">
                <input type="hidden" name="id" value="${res.QuestionID}">
                <button id="submit" type="submit">
            </form>`;
        }, res);
        
        window.click("button#submit");
        await window.waitForNavigation({
            timeout: 0,
        });
        const frame = await window.$("div.image-frame");
        if(frame == null) throw ""
        await frame.evaluate(frame => {
            frame.style.width = "644px";
            frame.style.height = "327px";
            frame.style.textAlign = "center";
            frame.style.verticalAlign = "middle";
            frame.style.display = "table";
        });
        const screenshotPath = `./screenshots/${res.Code}.png`;
        // TODO: move most of the logic outside the evaluated callback
        const question = await window.evaluate((res, screenshotPath, assets, baseUrl, answerPrefixes) => {
            const answers = [...document.querySelectorAll(".answer-container > .answer")]
            .map((e, i) => /** @type {[Element, number]} */([e, i]))
            .sort(([a, _], [b, __]) => {
                return (b.getAttribute("data-answerid") == res.CorrectAnswers[0]?.toString() ? 1 : 0)
                - (a.getAttribute("data-answerid") == res.CorrectAnswers[0]?.toString() ? 1 : 0);
            })
            .map(([e, i]) => (answerPrefixes[i] ?? "") + e.querySelector("p")?.textContent?.trim());

            const frameElements = document.querySelectorAll("div.image-frame > *");
            const videoSource = document.querySelector("div.image-frame > video > source");
            let mediaSrc;
            if (frameElements.length == 1) {
                const src = (frameElements[0] ?? videoSource)?.getAttribute("src");
                mediaSrc = src == null ? null : new URL(src, baseUrl).href;
            } else {
                mediaSrc = screenshotPath;
            }

            /** @type {Question} */
            return ({
                name: [...document.querySelectorAll(".question-text")]
                    .find(e => e.textContent?.trim() != "")
                    ?.textContent?.trim() ?? (()=>{throw ""})(),
                code: res.Code,
                id: res.QuestionID,
                media: mediaSrc ? assets[mediaSrc] ?? mediaSrc : undefined,
                answer: answers[0] ?? (()=>{throw ""})(),
                firstWrong: answers[1] ?? undefined,
                secondWrong: answers[2] ?? undefined,
            });
        }, res, screenshotPath, assets, baseUrl, answerPrefixes);
        if(question.media == screenshotPath) {
            stdout.write(`screenshotting ${screenshotPath} \x1B[0K`);
            await mkdir(`./screenshots`, { recursive: true, });
            await frame.screenshot({
                path: screenshotPath,
            });
        }
        questions.push(question);
    }
    const csvName = `./scrape.${section.id}.${section.name.slice(0, 50).replaceAll("/", "-")}.csv`;
    await writeFile(csvName, stringify(questions, {
        columns: [
            "id",
            "code",
            "name",
            "answer",
            "firstWrong",
            "secondWrong",
            "media",
        ],
    }), {
        flag: "w+",
    });
    console.log(`\r  ${questionResponses.Questions.length} questions written to ${csvName} \x1B[0K`);
}
await browser.close();
console.log(`done`);

/** @typedef {{id: number,
 * code: string,
 * name: string,
 * media?: string,
 * answer: string,
 * firstWrong?: string,
 * secondWrong?: string
 * }} Question */
/** @typedef {{Questions: {QuestionID: number, Code: string, CorrectAnswers: number[]}[]}} QuestionsRes */
/** @typedef {{name: string, id: string}} Section */
