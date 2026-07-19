use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::email::branding::{EmailTheme, stylesheet};

use super::footer::email_footer;
use super::header::email_header;

pub struct EmailLayout<'a> {
    subject: &'a str,
    preheader: &'a str,
    heading: Option<&'a str>,
    unsubscribe_link: Option<&'a str>,
    theme: &'a EmailTheme<'a>,
}

impl<'a> EmailLayout<'a> {
    pub fn new(subject: &'a str, theme: &'a EmailTheme<'a>) -> Self {
        Self {
            subject,
            preheader: subject,
            heading: None,
            unsubscribe_link: None,
            theme,
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
        let style = stylesheet(self.theme.branding);

        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (self.subject) }
                    style { (PreEscaped(style)) }
                }
                body {
                    div class="preheader" { (self.preheader) }
                    table role="presentation" width="100%" cellpadding="0" cellspacing="0" class="bg" {
                        tr {
                            td align="center" {
                                table role="presentation" width="100%" cellpadding="0" cellspacing="0" class="shell" {
                                    (email_header(self.theme))
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
                                    (email_footer(self.theme, self.unsubscribe_link))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
