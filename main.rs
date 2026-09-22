use std::{collections::{HashMap, HashSet}, io::{self, BufRead}};
use std::str::FromStr;

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

#[derive(Debug, PartialEq, Eq)]
pub struct HereDocuments {
    pub command: String,
    pub body: Option<String>,
}

#[derive(Debug)]
pub struct ShellState {
    pub pwd: String,
    pub oldpwd: Option<String>,
}

#[derive(Debug)]
pub struct ShellVariable {
    pub record: HashMap<String, String>,
}

#[derive(Debug)]
pub struct FileSystem {
    pub files: HashSet<String>,
}

#[derive(Debug)]
pub struct CommandSubstitution {
    pub record: HashMap<String, String>,
}

#[derive(Debug)]
pub struct ShellDrivenEvents {
    pub parent_pid: i32,
    pub child_pid: i32,
    pub execution: HashMap<i32, String>,
    pub status: HashMap<i32, String>,
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
                        // Checks for '-' char after '<<' token discover - improves HereDocuments
                        if op == "<<" && chars.peek() == Some(&'-') {
                            op.push(chars.next().unwrap());
                        }
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

pub fn parse_here_documents(lines: &[String]) -> Result<Vec<HereDocuments>, &'static str> {
    let mut results = Vec::new();
    let mut line_iter = lines.iter();

    while let Some(line) = line_iter.next() {
        let tokens = tokenize(line)?;
        if tokens.is_empty() {
            continue;
        }

        // Check if current line is heredoc command
        if let Some((op_idx, strip_tabs)) = find_heredoc_operator(&tokens) {
            let end_token  = match tokens.get(op_idx + 1) {
                Some(t) => t.clone(),
                None => return Err("ERR missing heredoc end token"),
            }; 

            // Now that command of heredoc is computed, we iterate through its body
            let mut body_lines = Vec::new();
            let mut find_end = false;

            for body_line in line_iter.by_ref() {
                let check_line = if strip_tabs {
                    body_line.trim_start_matches('\t')
                } else {
                    body_line
                };

                if check_line == end_token {
                    find_end = true;
                    break;
                }

                body_lines.push(check_line.to_string());
            }

            if !find_end {
                return Err("ERR unterminated heredoc boddy");
            }

            results.push(HereDocuments {
                command: line.trim().to_string(),
                body: {
                    if !body_lines.is_empty() {
                        Some(body_lines.join("\n"))
                    } else {
                        None
                    }
                },
            });

        } else {
            results.push(HereDocuments {
                command: line.trim().to_string(),
                body: None
            });
        }
    }
    Ok(results)
}

impl ShellState {
    pub fn new() -> Self {
        Self {
            pwd: INITIAL_PWD.to_string(),
            oldpwd: None,
        }
    }

    pub fn cd(&mut self, target: Option<&str>) -> Result<String, &'static str> {
        let destination = match target {
            None | Some("") | Some("~") => INITIAL_PWD.to_string(),
            Some("-") => match &self.oldpwd {
                Some(old) => old.clone(),
                None => return Err("cd: OLDPWD not set")
            },
            Some(path) => self.resolve_path(path),
        };

        self.oldpwd = Some(self.pwd.clone());
        self.pwd = destination;

        Ok(self.pwd.clone())
    }

    // Resolves virtual relative, '.', '..' paths
    fn resolve_path(&self, raw_path: &str) -> String {
        let absolute_path = if raw_path.starts_with('/') {
            raw_path.to_string()
        } else {
            format!("{}/{}", self.pwd, raw_path)
        };

        let mut components = Vec::new();

        for part in absolute_path.split('/') {
            match part {
                "" | "." => continue,
                ".." => {
                    components.pop();
                }
                segment => components.push(segment),
            }
        }
        format!("/{}", components.join("/"))
    }
}

impl ShellVariable {
    pub fn new() -> Self {
        Self {
            record: HashMap::new(),
        }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        // If key already existis in variables records, delete it - BASE CASE
        if self.record.contains_key(name) {
            self.record.remove(name);
        } else {
            self.record.insert(name.to_string(), value.to_string());
        }
    }

    pub fn unset(&mut self, name: &str) {
        self.record.remove(name);
    }
    
    // Expands registered shell variables in line param
    pub fn expand(&mut self, line: &str) -> Result<String, &'static str> {
        let mut result = String::new();
        let mut chars = line.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                // Check if '$' is followed by something to expand
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == '{' {
                        chars.next(); // Consumes '{'
                        let expanded_braces = self.expanded_braces(&mut chars)?;
                        result.push_str(&expanded_braces);
                        continue;
                    } else if next_ch.is_alphanumeric() || next_ch == '_' {
                        let expanded = self.expand_simple(&mut chars);
                        result.push_str(&expanded);
                        continue;
                    }
                }
            }
            result.push(ch);
        }
        // Remove the EXPAND reserved word from the command
        Ok(result.chars().skip("EXPAND ".len()).collect())
    }

    fn expanded_braces<I>(&self, chars: &mut std::iter::Peekable<I>) -> Result<String, &'static str>
    where
        I: Iterator<Item = char>
    {
        let mut expr = String::new();
        let mut closed = false;

        while let Some(ch) = chars.next() {
            if ch == '}' {
                closed = true;
                break;
            }
            expr.push(ch);
        }

        if !closed {
            return Err("ERR bad substitution: missing closing '}'");
        }

        // Case 1: ${#X} length of X - 0 if it's unset
        if expr.starts_with('#') {
            let var_name = &expr[1..];
            let val = self.record.get(var_name).map(|v| v.as_str()).unwrap_or("");
            return Ok(val.len().to_string());
        }

        // Case 2: ${X:-d} Value of "X" or 'd' if unset/empty
        if let Some(pos) = expr.find(":-") {
            let var_name = &expr[..pos];
            let default_val = &expr[pos + 2..];
            let val = self.record.get(var_name).map(|v| v.as_str()).unwrap_or("");
            return if val.is_empty() {
                Ok(default_val.to_string())
            } else {
                Ok(val.to_string())
            };
        }

        // Case 3: ${X} - Standard value lookup
        let val = self.record.get(&expr).map(|v| v.as_str()).unwrap_or("");
        Ok(val.to_string())
    }

    fn expand_simple<I>(&self, chars: &mut std::iter::Peekable<I>) -> String
    where 
        I: Iterator<Item = char>
    {
        let mut var_name = String::new();

        while let Some(&ch) = chars.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                var_name.push(chars.next().unwrap());
            } else {
                break;
            }
        }
        self.record.get(&var_name).cloned().unwrap_or_default()
    }
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            files: HashSet::new(),
        }
    }

    pub fn create_file(&mut self, filename: &str) {
        self.files.insert(filename.to_string());
    }

    pub fn match_filenames(&self, expression: &str) -> String {
        let mut matches: Vec<String> = self
            .files
            .iter()
            .filter(|filename| self.is_match(expression, filename))
            .cloned()
            .collect();
        
        // No file matches with given expression, we print expression
        if matches.is_empty() {
            return expression.to_string();
        }

        matches.sort();
        matches.join(" ")
    }

    // Recursive match engine
    fn is_match(&self, expression: &str, filename: &str) -> bool {
        // Hidden filenames cannot match wildcard expressions unless expression explicity starts
        // with '.'
        if filename.starts_with(".") && !expression.starts_with(".") {
            return false;
        }

        self.match_chars(&expression.chars().collect::<Vec<_>>(), &filename.chars().collect::<Vec<_>>())
    }

    fn match_chars(&self, pat: &[char], text: &[char]) -> bool {
        match (pat.first(), text.first()) {
            // Both empty -> match
            (None, None) => true,

            // Patter empty, text remeaning -> fail
            (None, Some(_)) => false,

            // Patter starts with "*"
            (Some(&'*'), _) => {
                self.match_chars(&pat[1..], text)
                    || (!text.is_empty() && text[0] != '/' && self.match_chars(pat, &text[1..]))
            }

            // Text expty but patter remains (and isnt *) -> fails
            (_, None) => false,

            // Patter starts with "?"
            (Some(&'?'), Some(&t_ch)) => {
                if t_ch == '/' { // No directory detection
                    false
                } else {
                    self.match_chars(&pat[1..], &text[1..])
                }
            }

            (Some(&'['), Some(&t_ch)) => {
                if let Some(close_idx) = pat.iter().position(|&c| c == ']') {
                    let class_pat = &pat[1..close_idx];
                    if self.match_char_class(class_pat, t_ch) {
                        self.match_chars(&pat[close_idx + 1..], &text[1..])
                    } else {
                        false
                    }
                } else {
                    // Malformed class without ']', treat as literal '['
                    pat[0] == t_ch && self.match_chars(&pat[1..], &text[1..])
                }
            } 

            // Literal Character match
            (Some(&p_ch), Some(&t_ch)) => {
                p_ch == t_ch && self.match_chars(&pat[1..], &text[1..])
            }
        }
    }

    // Helper for [abc], [!abc], and [a-z]
    fn match_char_class(&self, class_spec: &[char], ch: char) -> bool {
        if class_spec.is_empty() {
            return false;
        }

        let (negated, spec) = if class_spec[0] == '!' {
            (true, &class_spec[1..])
        } else {
            (false, class_spec)
        };

        let mut matched = false;
        let mut i = 0;

        while i < spec.len() {
            // Check for range pattern [a-z]
            if i + 2 < spec.len() && spec[i + 1] == '-' {
                let start = spec[i];
                let end = spec[i + 2];
                if ch >= start && ch <= end {
                    matched = true;
                    break;
                }
                i += 3;
            } else {
                if spec[i] == ch {
                    matched = true;
                    break;
                }
                i += 1;
            }
        }

        if negated { !matched } else { matched }
    }
}

impl CommandSubstitution {
    pub fn new() -> Self {
        Self {
            record: HashMap::new(),
        }
    }

    pub fn create(&mut self, var_name: &str, var_value: &str) {
        self.record.insert(var_name.to_string(), var_value.to_string());
    }

    pub fn expand(&self, line: &str) -> String {
        // Strips EXPAND if present in input
        let input = if line.starts_with("EXPAND ") {
            &line["EXPAND ".len()..]
        } else {
            line
        };
        
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;


        while i < chars.len() {
            // Command substitution
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
                i += 2; // Skip "$("

                // Finding matching closing parenthesis
                let start = i;
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '(' {
                        depth += 1;
                    } else if chars[i] == ')' {
                        depth -= 1;
                    }

                    if depth > 0 {
                        i += 1;
                    }
                }

                let inner_cmd: String = chars[start..i].iter().collect();
                if i < chars.len() && chars[i] == ')' {
                    i += 1; // Consume ')'
                }
                // Recursion - Expand inside-out first in case the inside contains expandable variables
                let expanded_inner = self.expand(&inner_cmd);

                // Execute inside command
                let cmd_output = self.execute_cmd(&expanded_inner);

                result.push_str(cmd_output.trim_end_matches('\n'));
                continue;
            }

            // Braces variables ${X}
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                i += 2;
                let start = i;
                
                while i < chars.len() && chars[i] != '}' {
                    i += 1;
                }

                let var_name: String = chars[start..i].iter().collect();
                if i <  chars.len() {
                    i += 1; // skip '}'
                }

                let val = self.record.get(&var_name).cloned().unwrap_or_default();
                result.push_str(&val);
                continue;
            }

            // Literals variables
            if chars[i] == '$' && i + 1 < chars.len() && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_') {
                i += 1; // Skip '$'
                let start = i;

                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }

                let var_name: String = chars[start..i].iter().collect();
                let val = self.record.get(&var_name).cloned().unwrap_or_default();
                result.push_str(&val);
                continue;
            }

            result.push(chars[i]);
            i += 1;
        }
        result
    }

    fn execute_cmd(&self, cmd: &str) -> String {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let mut parts = trimmed.splitn(2, ' ');
        let command_name = parts.next().unwrap_or("");
        let args = parts.next().unwrap_or("");

        match command_name {
            "echo" => args.to_string(),
            "upper" => args.to_uppercase(),
            "len" => args.len().to_string(),
            "cat" => {
                // cat ${X} or $X -> expand the variable
                if args.starts_with("$") {
                    self.expand(args)
                } else {
                    self.record.get(args).cloned().unwrap_or_default()
                }
            }
            _ => String::new()
        }
    }
}

impl ShellDrivenEvents {
    pub fn new() -> Self {
        Self {
            parent_pid: 0,
            child_pid: 0,
            execution: {
                let mut map = HashMap::new();
                map.insert(0, "shell".to_string());
                map
            },
            status: {
                let mut map = HashMap::new();
                map.insert(0, "running".to_string());
                map
            },
        }
    }

    pub fn fork(&mut self, parent_pid: i32, child_pid: i32) {
        self.parent_pid = parent_pid;
        self.child_pid = child_pid;
        // Child init
        self.execution.insert(child_pid, "shell".to_string());
        self.status.insert(child_pid, "running".to_string());
    }

    pub fn exec(&mut self, pid: i32, program: &str) {
        if self.execution.iter().find(|&(k, _)| *k == pid).is_some() {
            self.execution.insert(pid, program.to_string());
        }
    }

    pub fn exit(&mut self, pid: i32, code: i32) -> i32 {
        if self.execution.iter().find(|&(k, _)| *k == pid).is_some() {
            if let Some((_, status)) = self.status.iter().find(|&(k , _)| *k == pid) {
                if status.as_str() == "running" {
                    self.status.insert(pid, "zombie".to_string());
                }
            }
        }

        code
    }

    pub fn wait(&mut self, parent_pid: i32, child_pid: i32, exit_code: Option<i32>) -> Result<i32, &'static str> {
        if self.parent_pid == parent_pid && self.child_pid == child_pid && 
            self.status.get(&child_pid).map(|s| s.as_str()) != Some("reaped"){
            match exit_code {
                Some(code) => {
                    self.status.insert(child_pid, "reaped".to_string());
                    Ok(code)
                },
                None => Ok(-1)
            }
        } else {
            Ok(-1)
        }
    }

    pub fn status(&self, pid: i32) -> Result<String, &'static str> {
        if let Some((_, pid_status)) = self.status.iter().find(|&(k, _)| *k == pid) {
            if let Some((_, pid_program)) = self.execution.iter().find(|&(k, _)| *k == pid) {
                Ok(format!("{} prog={}", pid_status, pid_program))
            } else {
                Err("ERR: pid={pid} doesn't have program registry")
            }
        } else {
            Ok("unknown prog=?".to_string())
        }
    }
}

// Helper that identifies if a line is a heredoc command, if it is, returns its index + if it will
// be tabbed
pub fn find_heredoc_operator(tokens: &[String]) -> Option<(usize, bool)> {
    for (i, t) in tokens.iter().enumerate() {
        if t == "<<" {
            return Some((i, false));
        } else if t == "<<-" {
            return Some((i, true));
        }
    }
    None
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

// CONST DEFINITIONS
const INITIAL_PWD: &str = "/home/user";

fn main() {
    let stdin = io::stdin();
    
    // HERE DOCS RUN
    /*let lines: Vec<String> = match stdin.lock().lines().collect() {
        Ok(l) => l,
        Err(e) => {
            eprint!("Error reading stdin: {}", e);
            return;
        }
    };

    match parse_here_documents(&lines) {
        Ok(here_docs) => {
            for here_doc in here_docs {
                println!("CMD {}", here_doc.command);
                if let Some(body) = here_doc.body {
                    println!("BODY:\n{}\nEND", body);
                } else {
                    println!("BODY:\nEND");
                }
            }
        },
        Err(e) => eprint!("Error: {}", e),
    }*/
    //let mut state = ShellState::new();
    //let mut shell_variables=  ShellVariable::new();
    //let mut file_system = FileSystem::new();
    //let mut command_substitution = CommandSubstitution::new();
    let mut shell_events = ShellDrivenEvents::new();
    let mut exit_code: Option<i32> = None;
    for line in stdin.lock().lines() {
        let l = line.unwrap();
        if l.is_empty() { continue; }
       
        // PWD - CD command
        /*let mut command_input = l.split_whitespace(); 
        let command_name = command_input.next();
        let command_ops = command_input.next();

        match command_name {
            Some("pwd") => println!("{}", state.pwd),
            Some("cd") => match state.cd(command_ops) {
                Ok(new_pwd) => println!("{}", new_pwd),
                Err(e) => println!("{}", e),
            },
            _ => {}
        }*/
        
        let tokens = tokenize(&l).ok();
        
        /*if let Some(token_arr) = tokens {
            if let Some(idx) = token_arr.iter().position(|t| t == "SET") {
                if idx + 2 <= token_arr.len() {
                    shell_variables.set(token_arr[idx + 1].as_str(), token_arr[idx + 2].as_str());
                }
            } else if let Some(idx) = token_arr.iter().position(|t| t == "UNSET") {
                shell_variables.unset(token_arr[idx + 1].as_str());
            } else {
                println!("{}", shell_variables.expand(&l).unwrap());
            }
        }*/

        /*if let Some(token_arr) = tokens {
            if let Some(idx) = token_arr.iter().position(|t| t == "FILE") {
                file_system.create_file(token_arr[idx + 1].as_str());
            } 
                
            if let Some(idx) = token_arr.iter().position(|t| t == "MATCH") {
                println!("{}", file_system.match_filenames(token_arr[idx + 1].as_str()));
            }
        }*/

        /*if let Some(token_arr) = tokens {
            if let Some(idx) = token_arr.iter().position(|t| t == "SET") {
                if idx + 2 <= token_arr.len() {
                    command_substitution.create(token_arr[idx + 1].as_str(), token_arr[idx + 2].as_str());
                }
            }

            if let Some(_) = token_arr.iter().position(|t| t == "EXPAND") {
                print!("{}", command_substitution.expand(&l));
            }
        }*/
        
        if let Some(token_arr) = tokens {
            match token_arr[0].as_str() {
                "FORK" => {
                    if !token_arr[1].is_empty() && !token_arr[2].is_empty() {
                        shell_events.fork(FromStr::from_str(token_arr[1].as_str()).unwrap(),
                            FromStr::from_str(token_arr[2].as_str()).unwrap());
                    }
                },
                "EXEC" => {
                    if !token_arr[1].is_empty() && !token_arr[2].is_empty() {
                        shell_events.exec(FromStr::from_str(token_arr[1].as_str()).unwrap(),
                            token_arr[2].as_str());
                    }
                },
                "EXIT" => {
                    if !token_arr[1].is_empty() && !token_arr[2].is_empty() {
                        exit_code = Some(shell_events.exit(FromStr::from_str(token_arr[1].as_str()).unwrap(), 
                            FromStr::from_str(token_arr[2].as_str()).unwrap()));
                    }
                },
                "WAIT" => {
                    if !token_arr[1].is_empty() && !token_arr[2].is_empty() {
                        match shell_events.wait(FromStr::from_str(token_arr[1].as_str()).unwrap(), 
                            FromStr::from_str(token_arr[2].as_str()).unwrap(), 
                            exit_code) {
                            Ok(code) => println!("{}", code),
                            Err(e) => print!("{}", e)
                        }
                    }
                },
                "STATUS" => {
                    if !token_arr[1].is_empty() {
                        match shell_events.status(FromStr::from_str(token_arr[1].as_str()).unwrap()) {
                            Ok(result) => println!("{}", result),
                            Err(e) => eprint!("{}", e)
                        }
                    }
                },
                _ => println!("ERR: not supported")
            }
        }
        
        /*match tokens {
            Ok(tok) => {
                let formatted_output: Vec<String> = tok
                    .into_iter()
                    .map(|t| format!("[{}]", t))
                    .collect();

                println!("{}", formatted_output.join(" "));

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

                // PARSE REDIRECTIONS
                /*let redirection_commands = parse_redirections(&tok);

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
                }*/
            },
            Err(e) => println!("{}", e),
        }*/
        
    }
}
