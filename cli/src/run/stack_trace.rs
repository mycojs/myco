use v8;
use crate::run::state::MycoState;

/// Format a complete stack trace with source mapping applied
pub fn format_stack_trace_with_source_maps(scope: &mut v8::HandleScope, stack_trace: &str) -> String {
    let lines: Vec<&str> = stack_trace.lines().collect();
    let mut mapped_lines = Vec::new();
    
    for line in lines {
        if line.trim().starts_with("at ") {
            // Parse the stack frame line
            if let Some(mapped_line) = map_stack_frame_line(scope, line) {
                mapped_lines.push(mapped_line);
            } else {
                mapped_lines.push(line.to_string());
            }
        } else {
            // Keep error message and other non-frame lines as-is
            mapped_lines.push(line.to_string());
        }
    }
    
    mapped_lines.join("\n")
}

/// Generate and format a stack trace from V8 StackTrace with source mapping
pub fn format_v8_stack_trace_with_source_maps(scope: &mut v8::HandleScope, stack_trace: v8::Local<v8::StackTrace>, skip_frames: usize) -> String {
    let mut trace_lines = Vec::new();
    
    for i in skip_frames..stack_trace.get_frame_count() {
        if let Some(frame) = stack_trace.get_frame(scope, i) {
            let function_name = frame.get_function_name(scope)
                .map(|name| name.to_rust_string_lossy(scope))
                .unwrap_or_else(|| "<anonymous>".to_string());
            
            let script_name = frame.get_script_name(scope)
                .map(|name| name.to_rust_string_lossy(scope))
                .unwrap_or_else(|| "<unknown>".to_string());
            
            let line_number = frame.get_line_number();
            let column_number = frame.get_column();
            
            // Try to map this frame using source maps
            let mapped_location = map_location_with_source_maps(scope, &script_name, line_number as u32, column_number as u32);
            
            if let Some((mapped_file, mapped_line, mapped_column)) = mapped_location {
                trace_lines.push(format!("    at {} ({}:{}:{})", function_name, mapped_file, mapped_line, mapped_column));
            } else {
                trace_lines.push(format!("    at {} ({}:{}:{})", function_name, script_name, line_number, column_number));
            }
        }
    }
    
    trace_lines.join("\n")
}

/// Helper function to map a single stack frame line
fn map_stack_frame_line(scope: &mut v8::HandleScope, line: &str) -> Option<String> {
    // Parse pattern: "    at functionName (file:///path/to/file.js:line:column)"
    // or "    at file:///path/to/file.js:line:column"
    
    let trimmed = line.trim();
    if !trimmed.starts_with("at ") {
        return None;
    }
    
    let rest = &trimmed[3..]; // Remove "at "
    
    // Look for the pattern (file:line:column) at the end
    if let Some(paren_start) = rest.rfind('(') {
        if let Some(paren_end) = rest.rfind(')') {
            if paren_end > paren_start {
                let function_part = &rest[..paren_start].trim();
                let location_part = &rest[paren_start + 1..paren_end];
                
                if let Some((mapped_file, mapped_line, mapped_column)) = parse_and_map_location(scope, location_part) {
                    return Some(format!("    at {} ({}:{}:{})", function_part, mapped_file, mapped_line, mapped_column));
                }
            }
        }
    } else {
        // Format: "    at file:line:column" (no function name)
        if let Some((mapped_file, mapped_line, mapped_column)) = parse_and_map_location(scope, rest) {
            return Some(format!("    at {}:{}:{}", mapped_file, mapped_line, mapped_column));
        }
    }
    
    None
}

/// Helper function to parse and map a location string
fn parse_and_map_location(scope: &mut v8::HandleScope, location: &str) -> Option<(String, u32, u32)> {
    // Parse pattern: "file:///path/to/file.js:line:column"
    let parts: Vec<&str> = location.rsplitn(3, ':').collect();
    if parts.len() >= 3 {
        if let (Ok(column), Ok(line)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            let file_part = parts[2];
            map_location_with_source_maps(scope, file_part, line, column)
        } else {
            None
        }
    } else {
        None
    }
}

/// Helper function to map a location using source maps
pub fn map_location_with_source_maps(scope: &mut v8::HandleScope, script_name: &str, line: u32, column: u32) -> Option<(String, u32, u32)> {
    let state_ptr = scope.get_data(0) as *const MycoState;
    if state_ptr.is_null() {
        return None;
    }
    
    let state = unsafe { &*state_ptr };
    
    // Look up the source map for this script
    if let Some(source_map) = state.source_maps.get(script_name) {
        // Convert to 0-indexed for source map lookup (V8 uses 1-indexed)
        let zero_based_line = if line > 0 { line - 1 } else { 0 };
        let zero_based_column = if column > 0 { column - 1 } else { 0 };
        
        // Look up the original location
        if let Some(token) = source_map.lookup_token(zero_based_line, zero_based_column) {
            let original_line = token.get_src_line();
            let original_column = token.get_src_col();
            
            // Check if the token is mapped (u32::MAX is sentinel for unmapped)
            if original_line != u32::MAX && original_column != u32::MAX {
                // Get the original source file name
                let original_file = if let Some(source) = token.get_source() {
                    source.to_string()
                } else {
                    script_name.to_string()
                };
                
                // Convert back to 1-indexed for display
                return Some((original_file, original_line + 1, original_column + 1));
            }
        }
    }
    
    None
} 