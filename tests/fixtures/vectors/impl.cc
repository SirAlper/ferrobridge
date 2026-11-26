#include "vectors/input.h"

#include <sstream>

namespace geo {

Sample::Sample(std::int32_t value) : value_(value) {}
std::int32_t Sample::value() const { return value_; }

std::int32_t sum(const std::vector<std::int32_t>& values) {
  std::int32_t total = 0;
  for (std::int32_t v : values) {
    total += v;
  }
  return total;
}

void append_halves(std::vector<double>& values) {
  const std::size_t n = values.size();
  for (std::size_t i = 0; i < n; ++i) {
    values.push_back(values[i] / 2.0);
  }
}

std::vector<std::string> split_words(const std::string& text) {
  std::istringstream in(text);
  std::vector<std::string> words;
  std::string word;
  while (in >> word) {
    words.push_back(word);
  }
  return words;
}

std::int32_t count_words(const std::vector<std::string>& words) {
  return static_cast<std::int32_t>(words.size());
}

std::vector<Sample> make_samples(std::int32_t count) {
  std::vector<Sample> samples;
  for (std::int32_t i = 0; i < count; ++i) {
    samples.emplace_back(i * 10);
  }
  return samples;
}

std::int32_t first_value(const std::vector<Sample>& samples) {
  return samples.at(0).value();
}

void flags(const std::vector<bool>&) {}
void nested(const std::vector<std::vector<int>>&) {}
void by_value(std::vector<int>) {}
void custom(const std::vector<int, Arena<int>>&) {}

}  // namespace geo
