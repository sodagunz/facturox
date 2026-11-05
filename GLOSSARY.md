# Local inventory and orders

The shop's products, their available quantities, and locally committed orders. An order is not a payment record or an authorized fiscal invoice.

## Language

**Product**:
A catalog item with a stable identity, name, optional barcode and current unit price.

**ProductQuantity**:
The nonnegative number of available whole units of a product.
_Avoid_: Stock, stock balance

**Inventory**:
The shop-wide collection of products, their ProductQuantities and accepted orders.

**Order**:
A collection of product lines with one captured unit price and a positive quantity per product. Committing an order immediately deducts its quantities from Inventory; the order cannot be accepted if any ProductQuantity would become negative.

**Draft order**:
An order being edited before commitment, with no effect on ProductQuantity. Discarding it does not reverse a committed order.
_Avoid_: Basket as a distinct business entity

**Order line**:
One product's identity, captured name and unit price, and whole-unit quantity within an order. A product appears at most once in an order.

**ProductQuantity adjustment**:
An explicit nonzero signed change to a product's available quantity, optionally accompanied by a reason.
_Avoid_: Absolute count, stock adjustment
