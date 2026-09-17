use std::{io::{self, BufRead}};

#[derive(Debug, PartialEq, Eq)]
pub struct Redirection {
    pub fd: i32,
    pub operand: String,
    pub target: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub argv: Vec<String>,
    pub redirections: Vec<Redirection>,
}


pub fn tokenize(input: &str) -> Result<Vec<String>, &'static str> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();

    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut is_escaped = false;
    let mut active_token = false;

    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if is_escaped {
            current_token.push(c);
            active_token = true;
            is_escaped = false;
            continue;
        }

        if in_single_quotes {
            if c == '\'' {
                in_single_quotes = false;
            } else {
                current_token.push(c);
            }
        } else if in_double_quotes {
            match c {
                '"' => in_double_quotes = false,
                '\\' => is_escaped = true,
                _ => current_token.push(c),
            }
        } else {
            // Outside quotes
            match c {
                '\\' => {
                    is_escaped = true;
                    active_token = true;
                }
                '\'' => {
                    in_single_quotes = true;
                    active_token = true;
                }
                '"' => {
                    in_double_quotes = true;
                    active_token = true;
                }
                '<' | '>' => {
                    if active_token {
                        tokens.push(std::mem::take(&mut current_token));
                        active_token = false;
                    }

                    let mut op = c.to_string();

                    // Check for double operators '>>' or '<<'
                    if chars.peek() == Some(&c) {
                        op.push(chars.next().unwrap());
                    }
                    // Checks for '>&1' or '>&2'
                    else if chars.peek() == Some(&'&') {
                        op.push(chars.next().unwrap()); // Consumes '&'
                        if let Some(&next_c) = chars.peek() {
                            if next_c.is_ascii_digit() {
                                op.push(chars.next().unwrap()); // Consumes digit
                            }
                        }
                    }

                    tokens.push(op); 
                }
                '|' => {
                    if active_token {
                        tokens.push(std::mem::take(&mut current_token));
                        active_token = false;
                    }
                    tokens.push("|".to_string());
                }
                _ if c.is_whitespace() => {
                    if active_token {
                        tokens.push(std::mem::take(&mut current_token));
                        active_token = false;
                    }
                }
                // Handle digits attached directly to redirection operators (e.g., '2' in '2>err.txt')
                '0'..='9'
                    if current_token.is_empty()
                        && match chars.peek() { 
                            Some(&'<') | Some(&'>') => true,
                            _ => false,
                } =>
                {
                    let mut op = c.to_string(); // starts with "2" or "1"
                    op.push(chars.next().unwrap()); // consume '<' or '>'

                    if chars.peek() == Some(&'>') {
                        op.push(chars.next().unwrap()); // 2>>
                    } else if chars.peek() == Some(&'&') {
                        op.push(chars.next().unwrap()); // 2>&
                        if let Some(&next_c) = chars.peek() {
                            if next_c.is_ascii_digit() {
                                op.push(chars.next().unwrap()); // 2>&1
                            }
                        }
                    }

                    tokens.push(op);
                    active_token = false;
                }
                _ => {
                    current_token.push(c);
                    active_token = true;
                }
            }
        }
    }

    if is_escaped {
        return Err("ERR trailing Backslashes");
    }

    if in_single_quotes || in_double_quotes {
        return Err("ERR unterminated quote");
    }

    if active_token {
        tokens.push(current_token);
    }

    Ok(tokens)
}
pub fn parse_pipelines(tokens: &[String]) -> Result<Vec<Vec<String>>, &'static str> {
    let mut list_of_command_pipeline: Vec<Vec<String>> = Vec::new();
    let mut current_command: Vec<String> = Vec::new();

    for token in tokens { // Iterates through detected tokens
        if token == "|" {
            if current_command.is_empty() { // Check if there was no command before | token
                return Err("ERR syntax error: empty command in pipeline");
            }
            list_of_command_pipeline.push(std::mem::take(&mut current_command)); // push
            // command into list of lists and clears current_command
        } else {
            current_command.push(token.clone()); // get the current command 
        }
    }

    if token_ended_with_pipe(tokens) && current_command.is_empty() { // Checks if there is no
        // command after | token.
        return Err("ERR syntax error: empty command in pipeline");
    }

    if !current_command.is_empty() { // If there was a command after |, pushes as a valid command
        // into list of lists
        list_of_command_pipeline.push(current_command);
    }

    Ok(list_of_command_pipeline)
}

pub fn parse_redirections(tokens: &[String]) -> Result<Command, &'static str> {
    let mut argv: Vec<String> = Vec::new();
    let mut redirections: Vec<Redirection> = Vec::new();
    let mut iter = tokens.iter().peekable(); // Iterator which let
    // you look to next elements without passing to them

    while let Some(token) = iter.next() {
        // Check if current token is a redirection
        if is_redirection_token(token) {
            let (fd, operand, target) = if token == "2>&1" { // Dedicated cases
                // - treatment apart
                (2, ">".to_string(), "&1".to_string())
            } else if token == "1>&2" {
                (1, ">".to_string(), "&2".to_string())
            } else {
                let (fd, operand) = parse_fd_with_op(token);
                let target = match iter.next() {
                    Some(t) => t.clone(),
                    None => return Err("ERR missing redirect target"),
                };
                (fd, operand, target)
            };

            redirections.push(Redirection { fd, operand, target });
        } else {
            argv.push(format!("'{}'", token.clone()));
        }
    }

    Ok( Command { argv, redirections })
}

// Helper to identify if last token is |
pub fn token_ended_with_pipe(tokens: &[String]) -> bool {
    tokens.last().map_or(false, |t| t == "|")
}

// Helper to idnetify redirection tokens
pub fn is_redirection_token(token: &str) -> bool {
    match token {
        "<" => true,
        "<<" => true,
        ">" => true,
        ">>" => true,
        "2>" => true,
        "2>>" => true,
        "2>&1" => true,
        "1>&2" => true,
        "&>" => true,
        "1>" => true,
        _ => false,
}
}

// Helper to match redirect operand with fd
pub fn parse_fd_with_op(token: &str) -> (i32, String) {
    match token {
        "<"    => (0, "<".to_string()),
        "<<"   => (0, "<<".to_string()),
        ">"    => (1, ">".to_string()),
        ">>"   => (1, ">>".to_string()),
        "1>"   => (1, ">".to_string()),
        "2>"   => (2, ">".to_string()),
        "2>>"  => (2, "2>>".to_string()),
        "2>&1" => (2, ">".to_string()),
        _      => (2, token.to_string()),
    }
}

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let l = line.unwrap();
        if l.is_empty() { continue; }

        let tokens = tokenize(&l);
        
        match tokens {
            Ok(tok) => {
                /*let formatted_output: Vec<String> = tok
                    .into_iter()
                    .map(|t| format!("[{}]", t))
                    .collect();

                println!("{}", formatted_output.join(" "));*/

                // PARSER PIPELINES
                //let pipeline_commands = parse_pipelines(&tok);

                /*match pipeline_commands {
                    Ok(list_commands) => {
                        let formatted_pipeline = list_commands
                            .into_iter()
                            .map(|p| p.join(" "))
                            .collect::<Vec<String>>()
                            .join(" | ");
                        println!("{}", formatted_pipeline);
                    },
                    Err(e) => println!("{}", e),
                }*/

                let redirection_commands = parse_redirections(&tok);

                match redirection_commands {
                    Ok(command) => {
                        let argv_formatted = format!("[{}]", command.argv.join(", "));

                        println!("argv={}", argv_formatted);

                        for redir in command.redirections {
                            let formatted_redir = format!("redir fd={} op={} target={}", 
                                redir.fd, 
                                redir.operand, 
                                redir.target
                            );

                            println!("{}", formatted_redir);
                        }
                    },
                    Err(e) => println!("{}", e),
                }
            },
            Err(e) => println!("{}", e),
        }
        
    }
}
