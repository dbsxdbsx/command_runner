import random

print("Guess the number!")

secret_number = random.randint(1, 100)

while True:
    print("Please input your guess:")

    guess = input()

    try:
        guess = int(guess)
    except ValueError:
        continue

    print(f"You guessed: {guess}")

    if guess < secret_number:
        print("Too small!")
    elif guess > secret_number:
        print("Too big!")
    else:
        print("You win!")
        break