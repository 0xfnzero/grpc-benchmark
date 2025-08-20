use colored::*;
use console::{style, Term};
use std::io::Write;

pub struct ColoredOutput {
    term: Term,
}

impl Default for ColoredOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl ColoredOutput {
    pub fn new() -> Self {
        Self {
            term: Term::stdout(),
        }
    }

    pub fn header(&self, text: &str) {
        println!("{}", style(text).cyan().bold());
        println!("{}", "=".repeat(text.len()).cyan());
    }

    pub fn subheader(&self, text: &str) {
        println!();
        println!("{}", style(text).yellow().bold());
        println!("{}", "-".repeat(text.len()).yellow());
    }

    pub fn success(&self, text: &str) {
        println!("{} {}", "✓".green().bold(), text.green());
    }

    pub fn error(&self, text: &str) {
        println!("{} {}", "✗".red().bold(), text.red());
    }

    pub fn warning(&self, text: &str) {
        println!("{} {}", "⚠".yellow().bold(), text.yellow());
    }

    pub fn info(&self, text: &str) {
        println!("{} {}", "ℹ".cyan().bold(), text.white());
    }

    pub fn metric(&self, label: &str, value: &str, unit: &str) {
        println!(
            "{}: {} {}",
            label.cyan(),
            value.bright_red(),
            unit.dimmed()
        );
    }

    pub fn table_header(&self, headers: &[&str]) {
        let col_widths = [16, 14, 14, 14]; // 调整列宽，增加数值列宽度
        
        print!("{}", "┌".cyan());
        for (i, _) in headers.iter().enumerate() {
            let width = col_widths.get(i).unwrap_or(&12);
            print!("{}", "─".repeat(*width).cyan());
            if i < headers.len() - 1 {
                print!("{}", "┬".cyan());
            }
        }
        println!("{}", "┐".cyan());
        
        print!("{}", "│".cyan());
        for (i, header) in headers.iter().enumerate() {
            let width = col_widths.get(i).unwrap_or(&12);
            print!(" {:^width$} {}", header.cyan().bold(), "│".cyan(), width = width - 2);
        }
        println!();
        
        print!("{}", "├".cyan());
        for (i, _) in headers.iter().enumerate() {
            let width = col_widths.get(i).unwrap_or(&12);
            print!("{}", "─".repeat(*width).cyan());
            if i < headers.len() - 1 {
                print!("{}", "┼".cyan());
            }
        }
        println!("{}", "┤".cyan());
    }

    pub fn table_row(&self, columns: &[&str]) {
        let col_widths = [16, 14, 14, 14]; // 调整列宽，增加数值列宽度
        
        print!("{}", "│".dimmed());
        for (i, col) in columns.iter().enumerate() {
            let width = col_widths.get(i).unwrap_or(&12);
            if i == 0 {
                // 第一列（端点名称）左对齐，加粗
                print!(" {:<width$} {}", col.white().bold(), "│".dimmed(), width = width - 2);
            } else {
                // 其他列右对齐，添加边距
                print!(" {:>width$} {}", col.white(), "│".dimmed(), width = width - 2);
            }
        }
        println!();
    }

    pub fn table_footer(&self, num_columns: usize) {
        let col_widths = [16, 14, 14, 14]; // 调整列宽，增加数值列宽度
        
        print!("{}", "└".cyan());
        for i in 0..num_columns {
            let width = col_widths.get(i).unwrap_or(&12);
            print!("{}", "─".repeat(*width).cyan());
            if i < num_columns - 1 {
                print!("{}", "┴".cyan());
            }
        }
        println!("{}", "┘".cyan());
    }

    pub fn progress_bar(&self, label: &str, current: usize, total: usize) {
        let percentage = (current as f64 / total as f64 * 100.0) as usize;
        let filled = percentage / 2;
        let bar = "█".repeat(filled) + &"░".repeat(50 - filled);
        
        print!("\r{}: [{}] {}%", 
               label.cyan(), 
               bar.green(), 
               percentage.to_string().bold());
        std::io::stdout().flush().unwrap();
        
        if current == total {
            println!();
        }
    }

    pub fn separator(&self) {
        println!("{}", "─".repeat(80).dimmed());
    }

    pub fn clear_line(&self) {
        print!("\r{}", " ".repeat(80));
        print!("\r");
        std::io::stdout().flush().unwrap();
    }

    pub fn endpoint_status(&self, url: &str, status: EndpointStatus) {
        let (icon, color) = match status {
            EndpointStatus::Connected => ("🟢", "green"),
            EndpointStatus::Connecting => ("🟡", "yellow"),
            EndpointStatus::Failed => ("🔴", "red"),
            EndpointStatus::Testing => ("🔵", "cyan"),
        };
        
        let colored_url = match color {
            "green" => url.green(),
            "yellow" => url.yellow(),
            "red" => url.red(),
            "cyan" => url.cyan(),
            _ => url.white(),
        };
        
        println!("{} {}", icon, colored_url);
    }

    pub fn benchmark_result(&self, endpoint: &str, latency: f64, throughput: f64, success_rate: f64) {
        println!();
        println!("{}", style(format!("📊 Results for {}", endpoint)).cyan().bold());
        println!("{}", "─".repeat(50).cyan());
        
        let _latency_color = if latency < 50.0 { "green" } else if latency < 100.0 { "yellow" } else { "red" };
        let _throughput_color = if throughput > 1000.0 { "green" } else if throughput > 500.0 { "yellow" } else { "red" };
        let _success_color = if success_rate > 95.0 { "green" } else if success_rate > 90.0 { "yellow" } else { "red" };
        
        self.metric("Latency", &format!("{:.2}", latency), "ms");
        self.metric("Throughput", &format!("{:.0}", throughput), "req/s");
        self.metric("Success Rate", &format!("{:.1}", success_rate), "%");
    }
}

#[derive(Debug, Clone)]
pub enum EndpointStatus {
    Connected,
    Connecting,
    Failed,
    Testing,
}

// Helper functions for quick access
pub fn header(text: &str) {
    ColoredOutput::new().header(text);
}

pub fn success(text: &str) {
    ColoredOutput::new().success(text);
}

pub fn error(text: &str) {
    ColoredOutput::new().error(text);
}

pub fn warning(text: &str) {
    ColoredOutput::new().warning(text);
}

pub fn info(text: &str) {
    ColoredOutput::new().info(text);
}