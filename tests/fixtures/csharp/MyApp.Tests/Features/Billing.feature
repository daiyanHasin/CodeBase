Feature: Billing
  Scenario: Total of one bill
    Given a bill of 5 taka
    When I ask for the total
    Then the total is 5
