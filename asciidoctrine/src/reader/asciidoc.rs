pub use crate::ast::*;
use crate::options::Opts;
use crate::util::{Env, Environment};
use crate::reader::*;
use crate::Result;
use pest::iterators::Pair;
use pest::Parser;

pub struct AsciidocReader {}

impl AsciidocReader {
  pub fn new() -> Self {
    AsciidocReader {}
  }
}

impl crate::Reader for AsciidocReader {
  fn parse<'a>(&self, input: &'a str, args: &Opts, env: &mut Env) -> Result<AST<'a>> {
    let ast = AsciidocParser::parse(Rule::asciidoc, input)?;

    let mut attributes = Vec::new();
    if let Some(path) = &args.input {
      if let Some(path) = path.to_str() {
        attributes.push(Attribute {
          key: "source".to_string(),
          value: AttributeValue::String(path.to_string()),
        });
      }
    }

    let mut elements = Vec::new();

    for element in ast {
      if let Some(element) = process_element(element, env) {
        elements.push(element);
      }
    }

    Ok(AST {
      content: input,
      elements,
      attributes,
    })
  }
}

#[derive(Parser, Debug, Copy, Clone)]
#[grammar = "reader/asciidoc.pest"]
pub struct AsciidocParser;

fn process_element<'a>(
  element: Pair<'a, asciidoc::Rule>,
  env: &mut Env,
) -> Option<ElementSpan<'a>> {
  let mut base = set_span(&element);

  let element = match element.as_rule() {
    Rule::delimited_block => Some(process_delimited_block(element, env)),
    Rule::header => {
      for subelement in element.into_inner() {
        match subelement.as_rule() {
          Rule::title => {
            if let Some(e) = process_title(subelement, base.clone()) {
              base = e;
            }
          }
          // We just take the attributes at the beginning
          // of the element.
          _ => {
            break;
          } // TODO improve matching
        }
      }
      // TODO
      Some(base)
    }
    Rule::title => process_title(element, base),
    Rule::title_block => {
      for subelement in element.into_inner() {
        match subelement.as_rule() {
          Rule::title => {
            if let Some(e) = process_title(subelement, base.clone()) {
              base = e;
            }
          }
          Rule::anchor => {
            base = process_anchor(subelement, base);
          }
          // We just take the attributes at the beginning
          // of the element.
          _ => {
            break;
          } // TODO improve matching
        }
      }
      Some(base)
    }
    Rule::paragraph => Some(process_paragraph(element)),
    Rule::table_cell => {
      Some(process_table_cell(&element, base, env, &DEFAULT_CELL_FORMAT))
    }
    Rule::list => {
      for subelement in element.into_inner() {
        if let Some(e) = process_element(subelement, env) {
          base = e;
        }
      }
      Some(base)
    }
    Rule::list_paragraph => Some(process_paragraph(element)),
    Rule::other_list_inline => Some(from_element(&element, Element::Text)),
    Rule::continuation => None,
    Rule::bullet_list => {
      base.element = Element::List(ListType::Bullet);

      for subelement in element.into_inner() {
        if let Some(e) = process_element(subelement, env) {
          base.children.push(e);
        }
      }

      Some(base)
    }
    Rule::bullet_list_element => {
      for subelement in element.into_inner() {
        match subelement.as_rule() {
          Rule::bullet => {
            base.element = Element::ListItem(subelement.as_str().trim().len() as u32);
          }
          Rule::list_element => {
            for subelement in subelement.into_inner() {
              if let Some(e) = process_element(subelement, env) {
                base.children.push(e);
              }
            }
          }
          _ => {
            base.children.push(set_span(&subelement));
          }
        }
      }

      Some(base)
    }
    Rule::numbered_list => {
      base.element = Element::List(ListType::Number);

      for subelement in element.into_inner() {
        if let Some(e) = process_element(subelement, env) {
          base.children.push(e);
        }
      }

      Some(base)
    }
    Rule::number_bullet_list_element => {
      for subelement in element.into_inner() {
        match subelement.as_rule() {
          Rule::number_bullet => {
            base.element = Element::ListItem(subelement.as_str().trim().len() as u32);
          }
          Rule::list_element => {
            for subelement in subelement.into_inner() {
              if let Some(e) = process_element(subelement, env) {
                base.children.push(e);
              }
            }
          }
          _ => {
            base.children.push(set_span(&subelement));
          }
        }
      }

      Some(base)
    }
    Rule::image_block => Some(process_image(element, base, env)),
    Rule::table_row => Some(process_table_row(element, base, env, &[DEFAULT_CELL_FORMAT])),
    Rule::table_cell => Some(process_table_cell(&element, base, env, &DEFAULT_CELL_FORMAT)),
    Rule::block => {
      for subelement in element.into_inner() {
        if let Some(e) = process_element(subelement, env) {
          base = e;
        }
      }
      Some(base)
    }
    Rule::inline => Some(process_inline(element, base)),
    Rule::EOI => None,
    _ => Some(base),
  };

  element
}

fn process_anchor<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::inline_anchor => {
        base = process_inline_anchor(element, base);
      }
      _ => (),
    };
  }
  base
}

fn process_inline_anchor<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::identifier => {
        base.attributes.push(Attribute {
          key: "anchor".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      // TODO Fehler abfangen und anzeigen
      _ => (),
    }
  }
  base
}

fn process_inline_attribute_list<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  for subelement in element.into_inner() {
    match subelement.as_rule() {
      Rule::attribute => {
        for subelement in subelement.into_inner() {
          match subelement.as_rule() {
            Rule::attribute_value => {
              // TODO Wir müssen unterschiedlich damit umgehen, ob ein oder mehrere
              // identifier existieren
              base
                .positional_attributes
                .push(AttributeValue::Ref(subelement.as_str()));
            }
            Rule::named_attribute => {
              let mut key = None;
              let mut value = None;

              for subelement in subelement.into_inner() {
                match subelement.as_rule() {
                  Rule::identifier => key = Some(subelement.as_str()),
                  Rule::attribute_value => {
                    value = Some(subelement.into_inner().concat());
                  }
                  // TODO Fehler abfangen und anzeigen
                  _ => (),
                }
              }

              base.attributes.push(Attribute {
                key: key.unwrap().to_string(),
                value: AttributeValue::String(value.unwrap()),
              });
            }
            // TODO Fehler abfangen und anzeigen
            _ => (),
          }
        }
      }
      // TODO Fehler abfangen und anzeigen
      _ => (),
    }
  }
  base
}

fn process_attribute_list<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::inline_attribute_list => {
        base = process_inline_attribute_list(element, base);
      }
      _ => (),
    };
  }
  base
}

fn process_blocktitle<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::line => {
        base.attributes.push(Attribute {
          key: "title".to_string(), // TODO
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      _ => (),
    };
  }
  base
}

fn process_delimited_block<'a>(
  element: Pair<'a, asciidoc::Rule>,
  env: &mut Env,
) -> ElementSpan<'a> {
  let mut base = set_span(&element);

  for subelement in element.into_inner() {
    match subelement.as_rule() {
      Rule::anchor => {
        base = process_anchor(subelement, base);
      }
      Rule::attribute_list => {
        base = process_attribute_list(subelement, base);
      }
      Rule::blocktitle => {
        base = process_blocktitle(subelement, base);
      }
      Rule::delimited_table => {
        base.element = Element::Table;
        base = process_inner_table(subelement, base, env);
      }
      Rule::delimited_comment => {
        base.element = Element::TypedBlock {
          kind: BlockType::Comment,
        };
        base = process_delimited_inner(subelement, base, env);
      }
      Rule::delimited_source => {
        base.element = Element::TypedBlock {
          kind: BlockType::Listing,
        };
        base = process_delimited_inner(subelement, base, env);
      }
      Rule::delimited_literal => {
        base.element = Element::TypedBlock {
          kind: BlockType::Listing,
        };
        base = process_delimited_inner(subelement, base, env);
      }
      Rule::delimited_example => {
        base.element = Element::TypedBlock {
          kind: BlockType::Example,
        };
        base = process_delimited_inner(subelement, base, env);
      }
      // We just take the attributes at the beginning
      // of the element.
      _ => {
        break;
      } // TODO improve matching
    }
  }

  base
}

fn process_delimited_inner<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
  env: &mut Env,
) -> ElementSpan<'a> {
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::delimited_inner => {
        if let Element::TypedBlock { kind: BlockType::Example } = base.element {
          let ast = AsciidocParser::parse(Rule::asciidoc, element.as_str()).unwrap();

          for element in ast {
            if let Some(e) = process_element(element, env) {
              base.children.push(e);
            }
          }
        }
        base.attributes.push(Attribute {
          key: "content".to_string(), // TODO
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      _ => (),
    };
  }
  base
}

fn process_title<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> Option<ElementSpan<'a>> {
  match element.as_rule() {
    Rule::title => {
      for subelement in element.into_inner() {
        match subelement.as_rule() {
          Rule::atx_title_style => {
            base.element = Element::Title {
              level: subelement.as_str().trim().len() as u32,
            };
          }
          Rule::setext_title_style => {
            let ch = subelement.as_str().chars().next().unwrap(); // TODO Check None?
            let level;

            match ch {
              '=' => {
                level = 1;
              }
              '-' => {
                level = 2;
              }
              '~' => {
                level = 3;
              }
              '^' => {
                level = 4;
              }
              _ => {
                base.element = Element::Error("Unsupported title formatting".to_string());
                break;
              }
            }
            base.element = Element::Title {
              level: level as u32,
            };
          }
          Rule::line => {
            base.attributes.push(Attribute {
              key: "name".to_string(),
              value: AttributeValue::Ref(subelement.as_str()),
            });
          }
          // We just take the attributes at the beginning
          // of the element.
          _ => {
            break; // TODO Error
          } // TODO improve matching
        }
      }
    }
    _ => (),
  };

  Some(base)
}

fn process_paragraph<'a>(element: Pair<'a, asciidoc::Rule>) -> ElementSpan<'a> {
  let mut base = from_element(&element, Element::Paragraph);

  for subelement in element.into_inner() {
    base.children.push(match subelement.as_rule() {
      Rule::other_inline | Rule::other_list_inline => from_element(&subelement, Element::Text),
      Rule::inline => process_inline(subelement.clone(), set_span(&subelement)),
      _ => set_span(&subelement),
    });
  }

  base
}

fn process_inline<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::link => {
        base = process_link(element, base);
      }
      Rule::xref => {
        base = process_xref(element, base);
      }
      Rule::monospaced => {
        base.element = Element::Styled;
        base.attributes.push(Attribute {
          key: "style".to_string(),
          value: AttributeValue::Ref("monospaced"),
        });

        if let Some(content) = concat_elements(element.clone(), Rule::linechar, "") {
          base.attributes.push(Attribute {
            key: "content".to_string(),
            value: AttributeValue::String(content),
          });
        };
        for subelement in element.into_inner() {
          match subelement.as_rule() {
            Rule::inline_anchor => {
              base = process_inline_anchor(subelement, base);
            }
            _ => (),
          }
        }
      }
      Rule::strong => {
        base.element = Element::Styled;
        base.attributes.push(Attribute {
          key: "style".to_string(),
          value: AttributeValue::Ref("strong"),
        });

        if let Some(content) = concat_elements(element, Rule::linechar, "") {
          base.attributes.push(Attribute {
            key: "content".to_string(),
            value: AttributeValue::String(content),
          });
        };
      }
      Rule::emphasized => {
        base.element = Element::Styled;
        base.attributes.push(Attribute {
          key: "style".to_string(),
          value: AttributeValue::Ref("em"),
        });

        if let Some(content) = concat_elements(element, Rule::linechar, "") {
          base.attributes.push(Attribute {
            key: "content".to_string(),
            value: AttributeValue::String(content),
          });
        };
      }
      _ => (),
    };
  }
  base
}

fn process_link<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  base.element = Element::Link;
  for element in element.into_inner() {
    match element.as_rule() {
      Rule::url => {
        base.attributes.push(Attribute {
          key: "url".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
        let element = element.into_inner().next().unwrap(); // TODO Fehler möglich?
        base.attributes.push(Attribute {
          key: "protocol".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      Rule::inline_attribute_list => {
        base = process_inline_attribute_list(element, base);
      }
      _ => {
        base.children.push(set_span(&element));
      }
    };
  }
  base
}

fn process_xref<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
) -> ElementSpan<'a> {
  base.element = Element::XRef;
  for element in element.clone().into_inner() {
    match element.as_rule() {
      Rule::identifier => {
        base.attributes.push(Attribute {
          key: "id".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      Rule::word => {}
      _ => (),
    };
  }

  if let Some(content) = concat_elements(element, Rule::word, " ") {
    base.attributes.push(Attribute {
      key: "content".to_string(),
      value: AttributeValue::String(content),
    });
  };

  base
}

fn process_image<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
  env: &mut Env,
) -> ElementSpan<'a> {
  base.element = Element::Image;
  for element in element.into_inner().flatten() {
    match element.as_rule() {
      Rule::url => {
        base.attributes.push(Attribute {
          key: "path".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      Rule::path => {
        base.attributes.push(Attribute {
          key: "path".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      Rule::inline_attribute_list => {
        base = process_inline_attribute_list(element, base);
      }
      _ => (),
    };
  }

  // TODO Prüfen ob eine inline Anweisung vorhanden ist und
  // falls ja, die Datei einlesen
  if let Some(value) = base.get_attribute("opts") {
    if value == "inline" {
      // TODO Die Datei einlesen
      if let Some(path) = base.get_attribute("path") {
        match env.read_to_string(path) {
          Ok(content) => {
            base.attributes.push(Attribute {
              key: "content".to_string(),
              value: AttributeValue::String(content),
            });
          }
          Err(e) => {
            error!("couldn't read content of image file {} ({})", path, e);
          }
        }
      } else {
        error!("There was no path of inline image defined");
      }
    }
  }

  base
}

fn process_inner_table<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
  env: &mut Env,
) -> ElementSpan<'a> {
  let row_format = base.get_attribute("cols").unwrap_or("");
  let cell_formats = parse_row_format(row_format);

  for element in element.into_inner() {
    match element.as_rule() {
      Rule::delimited_inner => {
        let ast = AsciidocParser::parse(Rule::table_inner, element.as_str()).unwrap();
        for element in ast {
          for subelement in element.into_inner() {
            if let Some(e) = process_element(subelement, env) {
              base.children.push(e);
            }
          }
        }
        base.attributes.push(Attribute {
          key: "content".to_string(),
          value: AttributeValue::Ref(element.as_str()),
        });
      }
      Rule::table_row => {
        let row = process_table_row(element, base.clone(), env, &cell_formats);
        base.children.push(row);
      }
      _ => (),
    };
  }
  base
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CellKind {
  Default,
  Asciidoc,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CellFormat {
    length: usize,
    kind: CellKind,
}

fn parse_row_format(input: &str) -> Vec<CellFormat> {
  input.split(',')
       .map(|fmt| {
           let parts: Vec<&str> = fmt.split('=').collect();
           let kind = match parts.get(0).unwrap_or(&"default") {
            &"a" => CellKind::Asciidoc,
            _ => CellKind::Default,
           };
           let length = parts.get(1).unwrap_or(&"1").parse::<usize>().unwrap_or(1);
           CellFormat { length, kind }
       })
       .collect()
}

fn process_inner_table(input: &str, cell_format: Vec<CellFormat>) ->  {
  
}

fn process_table_row<'a>(
  element: Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
  env: &mut Env,
  cell_formats: &[CellFormat],
) -> ElementSpan<'a> {
  base.element = Element::TableRow;

  for (cell_element, cell_format) in element.into_inner().zip(cell_formats.iter().cycle()) {
    let cell = process_table_cell(&cell_element, base.clone(), env, cell_format);
    base.children.push(cell);
  }

  base
}

static DEFAULT_CELL_FORMAT : CellFormat = CellFormat { length: 1, kind: CellKind::Default };

fn process_table_cell<'a>(
  element: &Pair<'a, asciidoc::Rule>,
  mut base: ElementSpan<'a>,
  _env: &mut Env,
  cell_format: &CellFormat,
) -> ElementSpan<'a> {
  base.element = Element::TableCell;

  base.content = element.clone()
    .into_inner()
    .find(|sub| sub.as_rule() == Rule::table_cell_content)
    .map_or("", |pair| pair.as_str())
    .trim();

  base
}

// Helper functions

fn concat_elements<'a>(
  element: Pair<'a, asciidoc::Rule>,
  filter: asciidoc::Rule,
  join: &str,
) -> Option<String> {
  let elements: Vec<_> = element
    .into_inner()
    .filter(|e| e.as_rule() == filter)
    .map(|e| e.as_str())
    .collect();

  if elements.len() > 0 {
    Some(elements.join(join))
  } else {
    None
  }
}

fn set_span<'a>(element: &Pair<'a, asciidoc::Rule>) -> ElementSpan<'a> {
  from_element(
    element,
    Element::Error(format!("Not implemented:{:?}", element)),
  )
}

fn from_element<'a>(rule: &Pair<'a, asciidoc::Rule>, element: Element<'a>) -> ElementSpan<'a> {
  let (start_line, start_col) = rule.as_span().start_pos().line_col();
  let (end_line, end_col) = rule.as_span().end_pos().line_col();

  ElementSpan {
    element,
    source: None, // TODO
    content: rule.as_str(),
    children: Vec::new(),
    attributes: Vec::new(),
    positional_attributes: Vec::new(),
    start: rule.as_span().start(),
    end: rule.as_span().end(),
    start_line,
    start_col,
    end_line,
    end_col,
  }
}

#[cfg(test)]
mod test {
  use pretty_assertions::assert_eq;

  use super::*;

  #[test]
  fn test_table() {
    let out = parse_row_format(r#"1,a"#);
    assert_eq!(out, vec![CellFormat{length:1,kind:CellKind::Default}, CellFormat{length:1,kind:CellKind::Asciidoc}]);
  }

  #[test]
  fn test_inner_table() {
    let out = parse_row_format(r#"1,a"#);
    assert_eq!(out, vec![CellFormat{length:1,kind:CellKind::Default}, CellFormat{length:1,kind:CellKind::Asciidoc}]);
  }
  

}