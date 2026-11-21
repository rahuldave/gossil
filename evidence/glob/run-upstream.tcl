# Assertion harness for unchanged Fossil glob.test; invokes only the Rust candidate.
set candidate [file normalize [lindex $argv 0]]
set failures 0
set assertions 0
proc test_setup {args} {}
proc test_cleanup {} {}
proc fossil {args} {
  set ::RESULT [exec $::candidate vcs {*}$args]
}
proc test {name condition} {
  incr ::assertions
  if {[uplevel 1 [list expr $condition]]} {
    puts "test $name OK"
  } else {
    incr ::failures
    puts "test $name FAILED"
  }
}
source [file join [file dirname [info script]] upstream-glob.test]
puts "***** Final results: $failures errors out of $assertions tests"
exit [expr {$failures != 0}]
