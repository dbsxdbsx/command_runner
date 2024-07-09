import sys

if __name__ == "__main__":
    for i in range(1,5):
        if i%2 == 0:
            print(f"[{i}]:error print.", file=sys.stderr)
        else:
            print(f"[{i}]:normal print.")