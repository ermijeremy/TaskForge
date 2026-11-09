import sys

brackets = sys.stdin.buffer.readline().removesuffix(b"\n")
stack = []
matching = {ord(")"): ord("("), ord("]"): ord("["), ord("}"): ord("{")}
for bracket in brackets:
    if bracket in b"([{":
        stack.append(bracket)
    elif not stack or stack.pop() != matching[bracket]:
        print("NO")
        break
else:
    print("NO" if stack else "YES")
