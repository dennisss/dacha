

Example final UI use-case:

- Calculator:
    - 2 text boxes and a button

- We have some high level element format with a render() method
    - render() is called with a &mut GraphicsContext
        - Can be used to draw primitives
        - Also need a method of 

- Elements should also be able to receive events:
    - Can be processed asyncronously
    - In the case of mouse clicks and moves, can propagate this down based on cached render trees
    - Most elements will have a static list of children (because they aren't smart)
    - Custom element types can create elements as they go.

