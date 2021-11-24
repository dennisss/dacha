
#include <iostream>
#include <chrono>
#include <random>

int main ()
{

  std::mt19937 gen(1234);
  for (int i = 0; i < 2000; i++) {
	std::cout << gen() << "\n";
  }




  std::mt19937 generator;




  int counts[247];
  for (int i = 0; i < 247; i++) {
	counts[i] = 0;
  }

  for (int i = 0; i < 10000000; i++) {
	unsigned int n = generator();
	counts[n % 247] += 1;
  }

  for (int i = 0; i < 247; i++) {
	std::cout << counts[i] << std::endl;
  }

  return 0;
}
