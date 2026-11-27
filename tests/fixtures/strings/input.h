#pragma once

#include <cstddef>
#include <string>

namespace text {

std::string greet(const std::string& name);
void append_exclamation(std::string& text);
// Geçerli UTF-8 olmayan baytlar: güvenli katmanda `Error::Utf8` olmalı.
std::string raw_bytes();

// Bu sürümde desteklenmeyenler.
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
