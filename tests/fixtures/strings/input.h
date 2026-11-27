#pragma once

#include <cstddef>
#include <string>

namespace text {

std::string greet(const std::string& name);
void append_exclamation(std::string& text);
// Bytes that are not valid UTF-8: must be `Error::Utf8` in the safe layer.
std::string raw_bytes();

// Not supported in this version.
std::size_t count_vowels(const std::string& text);
std::string upper(std::string text);

class Label {
 public:
  explicit Label(const std::string& text);
  const std::string& text() const;
  std::string shout() const;
  void set_text(const std::string& text);

 private:
  std::string text_;
};

}  // namespace text
