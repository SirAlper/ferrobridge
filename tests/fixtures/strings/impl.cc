#include "strings/input.h"

#include <cctype>

namespace text {

std::string greet(const std::string& name) { return "Merhaba, " + name + "!"; }
void append_exclamation(std::string& text) { text += "!"; }

std::size_t count_vowels(const std::string& text) {
  std::size_t count = 0;
  for (char c : text) {
    switch (std::tolower(static_cast<unsigned char>(c))) {
      case 'a': case 'e': case 'i': case 'o': case 'u':
        ++count;
        break;
      default:
        break;
    }
  }
  return count;
}

std::string upper(std::string text) {
  for (char& c : text) c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
  return text;
}

Label::Label(const std::string& text) : text_(text) {}
const std::string& Label::text() const { return text_; }
std::string Label::shout() const { return upper(text_); }
void Label::set_text(const std::string& text) { text_ = text; }

}  // namespace text
