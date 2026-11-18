def adjust(a, b):
    return a * 10 + b


class Holder:
    @property
    def value(self):
        print("read")
        return 2

    def run(self, n):
        return adjust(self.value, n)


holder = Holder()
print(holder.run(3))
print(adjust(holder.value, 4))
