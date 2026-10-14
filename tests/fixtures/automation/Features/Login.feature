Feature: Login
  Background:
    Given the browser is open
  Scenario: Driver logs in with valid details
    When I log in as "driver1"
    Then I see the dashboard
  Scenario: Wrong password is rejected
    When I log in as "baduser"
    Then I see the error "Invalid password"
    And I reset my password
