use maud::{html, Markup, PreEscaped, DOCTYPE};

use super::footer::email_footer;
use super::header::email_header;
use super::styles::STYLE;

pub struct EmailLayout<'a> {
    subject: &'a str,
    preheader: &'a str,
    heading: Option<&'a str>,
    unsubscribe_link: Option<&'a str>,
}

impl<'a> EmailLayout<'a> {
    pub fn new(subject: &'a str) -> Self {
        Self {
            subject,
            preheader: subject,
            heading: None,
            unsubscribe_link: None,
        }
    }

    pub fn preheader(mut self, preheader: &'a str) -> Self {
        self.preheader = preheader;
        self
    }

    pub fn heading(mut self, heading: &'a str) -> Self {
        self.heading = Some(heading);
        self
    }

    pub fn unsubscribe_link(mut self, link: Option<&'a str>) -> Self {
        self.unsubscribe_link = link;
        self
    }

    pub fn render(self, body: Markup, cta: Option<(&str, &str)>) -> Markup {
        let title = self.heading.unwrap_or(self.subject);

        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (self.subject) }
                    style { (PreEscaped(STYLE)) }
                }
                body {
                    div class="preheader" { (self.preheader) }
                    table role="presentation" width="100%" cellpadding="0" cellspacing="0" class="bg" {
                        tr {
                            td align="center" {
                                table role="presentation" width="100%" cellpadding="0" cellspacing="0" class="shell" {
                                    (email_header())
                                    tr {
                                        td class="panel" {
                                            h1 { (title) }
                                            (body)
                                            @if let Some((label, url)) = cta {
                                                p style="margin:24px 0 0;" {
                                                    a class="button" href=(url) { (label) }
                                                }
                                            }
                                        }
                                    }
                                    (email_footer(self.unsubscribe_link))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
