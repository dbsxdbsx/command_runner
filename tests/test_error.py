# test_error.py (1-17)
import sys


def main():
    # Attempt to execute an operation that might raise an exception
    try:
        result = 10 / 0 # Intentionally raise an exception
    except Exception as e:
        # Catch the exception and output the error information to stderr
        print(f"Error: {str(e)}", file=sys.stderr)

    # Continue executing, output normal information to stdout
    print("This is normal output information")
    print("The program continues to execute...")

if __name__ == "__main__":
    main()
