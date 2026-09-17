use std::{io::{self, BufRead}, thread::current};

pub fn tokenize(input: &str) -> Result<Vec<String>, &'static str> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();

    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut is_escaped = false;

    // Track if we are currently buildig a true token
    let mut active_token = false;

    for c in input.chars() {
        if is_escaped {
            // Backslashes escapes the next character
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
                '"'  => in_double_quotes = false,
                '\\' => is_escaped = true,
                _    => current_token.push(c),
            }
        } else {
            // Outside quotes
            match c {
                '\\' => { 
                    is_escaped = true; active_token = true; 
                }
                '\'' => { 
                    in_single_quotes = true; active_token = true; 
                }
                '"'  => { 
                    in_double_quotes = true; active_token = true; 
                }
                _ if c.is_whitespace() => {
                    if active_token {
                        tokens.push(std::mem::take(&mut current_token));
                        active_token = false;
                    }
                }
                _ => {
                    current_token.push(c); active_token = true;
                    if c == '|' {
                        tokens.push(std::mem::take(&mut current_token));
                        active_token = false;
                    }
                }
            }
        }
    }

    // Error control
    if is_escaped {
        return Err("ERR trailing Backslashes");
    }

    if in_single_quotes || in_double_quotes {
        return Err("ERR unterminated quote");
    }

    // Push final token if we where building one
    if active_token {
        tokens.push(current_token);
    }

    Ok(tokens)
}

pub fn parse(tokens: &[String]) -> Result<Vec<Vec<String>>, &'static str> {
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

pub fn token_ended_with_pipe(tokens: &[String]) -> bool {
    tokens.last().map_or(false, |t| t == "|")
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

                let pipeline_commands = parse(&tok);

                match pipeline_commands {
                    Ok(list_commands) => {
                        let formatted_pipeline = list_commands
                            .into_iter()
                            .map(|p| p.join(" "))
                            .collect::<Vec<String>>()
                            .join(" | ");
                        println!("{}", formatted_pipeline);
                    },
                    Err(e) => println!("{}", e),
                }
            },
            Err(e) => println!("{}", e),
        }
        
    }
}
