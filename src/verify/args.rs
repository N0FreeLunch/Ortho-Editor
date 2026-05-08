use std::collections::HashMap;
use std::ffi::OsString;
use std::env;

#[derive(Debug, Clone, PartialEq)]
pub enum ArgType {
    /// A positional argument (e.g., `file.txt`)
    Positional,
    /// Collects all remaining positional arguments
    RepeatedPositional,
    /// A boolean flag (e.g., `--verbose` or `-v`)
    Flag,
    /// A key-value option (e.g., `--output out.json` or `-o out.json`)
    Option,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArgError {
    Help(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub name: String,
    pub short: Option<char>,
    pub long: Option<String>,
    pub arg_type: ArgType,
    pub description: String,
    pub required: bool,
    pub value_count: usize,
    pub env_var: Option<String>,
    pub default: Option<String>,
}

impl ArgSpec {
    pub fn positional(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            short: None,
            long: None,
            arg_type: ArgType::Positional,
            description: description.to_string(),
            required: true,
            value_count: 1,
            env_var: None,
            default: None,
        }
    }

    pub fn repeated_positional(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            short: None,
            long: None,
            arg_type: ArgType::RepeatedPositional,
            description: description.to_string(),
            required: false,
            value_count: 0,
            env_var: None,
            default: None,
        }
    }

    pub fn flag(name: &str, short: Option<char>, long: Option<&str>, description: &str) -> Self {
        Self {
            name: name.to_string(),
            short,
            long: long.map(|s| s.to_string()),
            arg_type: ArgType::Flag,
            description: description.to_string(),
            required: false,
            value_count: 0,
            env_var: None,
            default: None,
        }
    }

    pub fn option(name: &str, short: Option<char>, long: Option<&str>, description: &str) -> Self {
        Self {
            name: name.to_string(),
            short,
            long: long.map(|s| s.to_string()),
            arg_type: ArgType::Option,
            description: description.to_string(),
            required: false,
            value_count: 1,
            env_var: None,
            default: None,
        }
    }

    pub fn short(&mut self, short: char) -> &mut Self {
        self.short = Some(short);
        self
    }

    pub fn long(&mut self, long: &str) -> &mut Self {
        self.long = Some(long.to_string());
        self
    }

    pub fn description(&mut self, description: &str) -> &mut Self {
        self.description = description.to_string();
        self
    }

    pub fn required(&mut self) -> &mut Self {
        self.required = true;
        self
    }

    pub fn optional(&mut self) -> &mut Self {
        self.required = false;
        self
    }

    pub fn with_env(&mut self, env_var: &str) -> &mut Self {
        self.env_var = Some(env_var.to_string());
        self
    }

    pub fn with_default(&mut self, default: &str) -> &mut Self {
        self.default = Some(default.to_string());
        self
    }

    pub fn default(&mut self, default: &str) -> &mut Self {
        self.default = Some(default.to_string());
        self
    }

    pub fn value_count(&mut self, count: usize) -> &mut Self {
        self.value_count = count;
        self
    }
}

#[derive(Debug)]
pub struct ArgParser {
    program_name: String,
    specs: Vec<ArgSpec>,
    auto_load_dotenv: bool,
    description: Option<String>,
}

#[derive(Debug)]
pub struct ParsedArgs {
    values: HashMap<String, Vec<String>>,
    flags: HashMap<String, bool>,
}

impl ArgParser {
    pub fn new(program_name: &str) -> Self {
        Self {
            program_name: program_name.to_string(),
            specs: Vec::new(),
            auto_load_dotenv: true,
            description: None,
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn disable_dotenv(mut self) -> Self {
        self.auto_load_dotenv = false;
        self
    }

    pub fn add_spec(&mut self, spec: ArgSpec) {
        self.specs.push(spec);
    }

    /// Adds a positional argument and returns a mutable reference for further configuration.
    pub fn add_arg(&mut self, name: &str) -> &mut ArgSpec {
        let spec = ArgSpec::positional(name, "");
        self.specs.push(spec);
        self.specs.last_mut().unwrap()
    }

    /// Adds a flag argument and returns a mutable reference for further configuration.
    pub fn add_flag(&mut self, name: &str) -> &mut ArgSpec {
        let spec = ArgSpec::flag(name, None, None, "");
        self.specs.push(spec);
        self.specs.last_mut().unwrap()
    }

    /// Adds an option argument and returns a mutable reference for further configuration.
    pub fn add_opt(&mut self, name: &str) -> &mut ArgSpec {
        let spec = ArgSpec::option(name, None, None, "");
        self.specs.push(spec);
        self.specs.last_mut().unwrap()
    }

    pub fn parse(&self, args: Vec<OsString>) -> Result<ParsedArgs, ArgError> {
        if self.auto_load_dotenv {
            let _ = dotenvy::dotenv();
        }
        let mut values = HashMap::new();
        let mut flags = HashMap::new();
        let mut positional_idx = 0;
        let mut it = args.into_iter().peekable();

        while let Some(arg) = it.next() {
            let arg_str = arg.to_string_lossy();
            if arg_str.starts_with("--") {
                let long_name = &arg_str[2..];
                if let Some(spec) = self.specs.iter().find(|s| s.long.as_deref() == Some(long_name)) {
                    match spec.arg_type {
                        ArgType::Flag => {
                            flags.insert(spec.name.clone(), true);
                        }
                        ArgType::Option => {
                            let mut collected = Vec::new();
                            for _ in 0..spec.value_count {
                                if let Some(val) = it.next() {
                                    collected.push(val.to_string_lossy().to_string());
                                } else {
                                    return Err(ArgError::Error(format!("Option --{} requires {} value(s)", long_name, spec.value_count)));
                                }
                            }
                            values.insert(spec.name.clone(), collected);
                        }
                        ArgType::Positional | ArgType::RepeatedPositional => unreachable!(),
                    }
                } else if long_name == "help" {
                    return Err(ArgError::Help(self.usage()));
                } else if long_name == "json" {
                    flags.insert("json".to_string(), true);
                } else if long_name == "antigravity" || long_name == "token-efficient" {
                    flags.insert("token-efficient".to_string(), true);
                } else if long_name == "output" {
                    if let Some(val) = it.next() {
                        values.insert("output".to_string(), vec![val.to_string_lossy().to_string()]);
                    } else {
                        return Err(ArgError::Error("Option --output requires 1 value".to_string()));
                    }
                } else {
                    return Err(ArgError::Error(format!("Unknown option --{}", long_name)));
                }
            } else if arg_str.starts_with("-") && arg_str.len() > 1 {
                let short_name = arg_str.chars().nth(1).unwrap();
                if let Some(spec) = self.specs.iter().find(|s| s.short == Some(short_name)) {
                    match spec.arg_type {
                        ArgType::Flag => {
                            flags.insert(spec.name.clone(), true);
                        }
                        ArgType::Option => {
                            let mut collected = Vec::new();
                            for _ in 0..spec.value_count {
                                if let Some(val) = it.next() {
                                    collected.push(val.to_string_lossy().to_string());
                                } else {
                                    return Err(ArgError::Error(format!("Option -{} requires {} value(s)", short_name, spec.value_count)));
                                }
                            }
                            values.insert(spec.name.clone(), collected);
                        }
                        ArgType::Positional | ArgType::RepeatedPositional => unreachable!(),
                    }
                } else if short_name == 'h' {
                    return Err(ArgError::Help(self.usage()));
                } else if short_name == 'o' {
                    if let Some(val) = it.next() {
                        values.insert("output".to_string(), vec![val.to_string_lossy().to_string()]);
                    } else {
                        return Err(ArgError::Error("Option -o requires 1 value".to_string()));
                    }
                } else if short_name == 't' {
                    // Check if it's -te
                    if arg_str == "-te" {
                         flags.insert("token-efficient".to_string(), true);
                    } else {
                         return Err(ArgError::Error(format!("Unknown option -{}", short_name)));
                    }
                } else {
                    return Err(ArgError::Error(format!("Unknown option -{}", short_name)));
                }
            } else {
                // Positional
                if let Some(spec) = self.specs.iter().filter(|s| matches!(s.arg_type, ArgType::Positional)).nth(positional_idx) {
                    values.insert(spec.name.clone(), vec![arg_str.to_string()]);
                    positional_idx += 1;
                } else if let Some(spec) = self.specs.iter().find(|s| matches!(s.arg_type, ArgType::RepeatedPositional)) {
                    values.entry(spec.name.clone()).or_insert_with(Vec::new).push(arg_str.to_string());
                } else {
                    return Err(ArgError::Error(format!("Unexpected positional argument: {}", arg_str)));
                }
            }
        }

        // Apply env fallbacks and defaults, check required
        for spec in &self.specs {
            if matches!(spec.arg_type, ArgType::Flag) {
                if !flags.contains_key(&spec.name) {
                    flags.insert(spec.name.clone(), false);
                }
                continue;
            }

            if !values.contains_key(&spec.name) {
                if let Some(env_name) = &spec.env_var {
                    if let Ok(val) = env::var(env_name) {
                        values.insert(spec.name.clone(), vec![val]);
                    }
                }
            }

            if !values.contains_key(&spec.name) {
                if let Some(default) = &spec.default {
                    values.insert(spec.name.clone(), vec![default.clone()]);
                }
            }

            if spec.required && !values.contains_key(&spec.name) {
                return Err(ArgError::Error(format!("Missing required argument: {}", spec.name)));
            }
        }

        Ok(ParsedArgs { values, flags })
    }

    pub fn usage(&self) -> String {
        let mut usage = String::new();
        if let Some(desc) = &self.description {
            usage.push_str(&format!("{}\n\n", desc));
        }
        usage.push_str(&format!("Usage: {}", self.program_name));
        let mut options_txt = String::new();

        for spec in &self.specs {
            match spec.arg_type {
                ArgType::Positional => {
                    if spec.required {
                        usage.push_str(&format!(" <{}>", spec.name));
                    } else {
                        usage.push_str(&format!(" [{}]", spec.name));
                    }
                }
                ArgType::RepeatedPositional => {
                    usage.push_str(&format!(" [{}...]", spec.name));
                }
                _ => {}
            }
        }

        usage.push_str(" [options]\n\nOptions:\n");

        for spec in &self.specs {
            let mut opt = String::new();
            if let Some(short) = spec.short {
                opt.push_str(&format!("-{}, ", short));
            } else {
                opt.push_str("    ");
            }

            if let Some(long) = &spec.long {
                opt.push_str(&format!("--{}", long));
            }

            match spec.arg_type {
                ArgType::Option => {
                    if spec.value_count > 1 {
                        opt.push_str(&format!(" <value1>...<value{}>", spec.value_count));
                    } else {
                        opt.push_str(" <value>");
                    }
                }
                ArgType::Positional | ArgType::RepeatedPositional => continue,
                ArgType::Flag => {}
            }

            options_txt.push_str(&format!("  {:20} {}\n", opt, spec.description));
        }
        
        options_txt.push_str("  -h, --help           Show this help message\n");
        options_txt.push_str("      --json           Output in machine-readable JSON format\n");
        options_txt.push_str("  -o, --output <PATH>  Save execution output to a file\n");
        options_txt.push_str("  -te, --token-efficient AI mode: save to antigravity/outputs/ and summarize\n");

        usage.push_str(&options_txt);
        usage
    }
}

impl ParsedArgs {
    pub fn get(&self, name: &str) -> Option<&String> {
        self.values.get(name).and_then(|v| v.first())
    }

    pub fn get_vec(&self, name: &str) -> Option<&Vec<String>> {
        self.values.get(name)
    }

    pub fn is_set(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    pub fn is_json(&self) -> bool {
        self.is_set("json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parsing() {
        let mut parser = ArgParser::new("test");
        parser.add_arg("input").description("input file");
        parser.add_flag("verbose").short('v').long("verbose").description("verbose output");
        parser.add_opt("output").short('o').long("output").description("output file");

        let args = vec![
            OsString::from("in.bin"),
            OsString::from("-v"),
            OsString::from("--output"),
            OsString::from("out.json"),
        ];

        let parsed = parser.parse(args).unwrap();
        assert_eq!(parsed.get("input").unwrap(), "in.bin");
        assert!(parsed.is_set("verbose"));
        assert_eq!(parsed.get("output").unwrap(), "out.json");
    }

    #[test]
    fn test_multi_value_option() {
        let mut parser = ArgParser::new("test");
        parser.add_opt("bits").long("bits").description("start and count").value_count(2);

        let args = vec![
            OsString::from("--bits"),
            OsString::from("64"),
            OsString::from("32"),
        ];

        let parsed = parser.parse(args).unwrap();
        let bits = parsed.get_vec("bits").unwrap();
        assert_eq!(bits.len(), 2);
        assert_eq!(bits[0], "64");
        assert_eq!(bits[1], "32");
    }

    #[test]
    fn test_repeated_positional() {
        let mut parser = ArgParser::new("test");
        parser.add_arg("main").description("main file");
        // We still need a way to set repeated positional via add_arg if we want it to be ergonomic
        // For now, let's update ArgSpec manually for this specific case if needed, 
        // or add add_repeated_arg
        let mut spec = ArgSpec::repeated_positional("extras", "extra files");
        parser.add_spec(spec);

        let args = vec![
            OsString::from("main.d2s"),
            OsString::from("extra1.d2s"),
            OsString::from("extra2.d2s"),
        ];

        let parsed = parser.parse(args).unwrap();
        assert_eq!(parsed.get("main").unwrap(), "main.d2s");
        let extras = parsed.get_vec("extras").unwrap();
        assert_eq!(extras.len(), 2);
        assert_eq!(extras[0], "extra1.d2s");
        assert_eq!(extras[1], "extra2.d2s");
    }

    #[test]
    fn test_missing_required() {
        let mut parser = ArgParser::new("test");
        parser.add_arg("input").description("input file");
        
        let args = vec![];
        let result = parser.parse(args);
        assert!(result.is_err());
        match result.unwrap_err() {
            ArgError::Error(e) => assert!(e.contains("Missing required argument")),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_new_dsl() {
        let mut parser = ArgParser::new("test");
        parser.add_arg("name").description("your name").optional().default("world");
        parser.add_flag("verbose").short('v').long("verbose");
        parser.add_opt("level").short('l').default("1");

        // Case 1: Missing optional with default
        let args = vec![];
        let parsed = parser.parse(args).unwrap();
        assert_eq!(parsed.get("name").unwrap(), "world");
        assert_eq!(parsed.get("level").unwrap(), "1");
        assert!(!parsed.is_set("verbose"));

        // Case 2: Explicit values
        let args = vec![
            OsString::from("Alice"),
            OsString::from("-v"),
            OsString::from("-l"),
            OsString::from("2"),
        ];
        let parsed = parser.parse(args).unwrap();
        assert_eq!(parsed.get("name").unwrap(), "Alice");
        assert_eq!(parsed.get("level").unwrap(), "2");
        assert!(parsed.is_set("verbose"));
    }

    #[test]
    fn test_defaults_and_env() {
        let mut parser = ArgParser::new("test");
        parser.add_opt("port").long("port").description("port number").with_default("8080");
        parser.add_opt("host").long("host").description("host name").with_env("TEST_HOST");

        unsafe {
            env::set_var("TEST_HOST", "localhost");
        }
        
        let args = vec![];
        let parsed = parser.parse(args).unwrap();
        assert_eq!(parsed.get("port").unwrap(), "8080");
        assert_eq!(parsed.get("host").unwrap(), "localhost");
    }
}

