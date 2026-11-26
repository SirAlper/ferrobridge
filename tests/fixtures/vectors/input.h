#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace geo {

class Sample {
 public:
  explicit Sample(std::int32_t value);
  std::int32_t value() const;

 private:
  std::int32_t value_;
};

// Değer semantiği: açıklama gerekmez.
std::int32_t sum(const std::vector<std::int32_t>& values);
void append_halves(std::vector<double>& values);
std::vector<std::string> split_words(const std::string& text);
std::int32_t count_words(const std::vector<std::string>& words);
std::vector<Sample> make_samples(std::int32_t count);
std::int32_t first_value(const std::vector<Sample>& samples);

// Reddedilmesi gerekenler.
void flags(const std::vector<bool>& bits);
void nested(const std::vector<std::vector<int>>& grid);
void by_value(std::vector<int> values);

template <typename T>
struct Arena {
  using value_type = T;
  T* allocate(std::size_t n);
  void deallocate(T* p, std::size_t n);
};
void custom(const std::vector<int, Arena<int>>& values);

}  // namespace geo
