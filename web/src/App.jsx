// Copyright 2018 Red Hat, Inc
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

import React from 'react'
import PropTypes from 'prop-types'
import { withRouter } from 'react-router'
import { Link, Redirect, Route, Switch } from 'react-router-dom'
import { Masthead } from 'patternfly-react'

import logo from './images/logo.png'
import { routes } from './routes'


class App extends React.Component {
  static propTypes = {
    info: PropTypes.object,
    location: PropTypes.object
  }

  constructor () {
    super()
    this.menu = routes()
  }

  renderMenu () {
    const { location } = this.props
    const activeItem = this.menu.find(
      item => location.pathname === item.to
    )
    return (
      <ul className="nav navbar-nav navbar-primary">
        {this.menu.filter(item => item.title).map(item => (
          <li key={item.to} className={item === activeItem ? 'active' : ''}>
            <Link to={item.to}>{item.title}</Link>
          </li>
        ))}
      </ul>
    )
  }

  renderContent = () => {
    const allRoutes = []
    this.menu.map((item, index) => {
      allRoutes.push(
        <Route key={index} exact
               path={item.to}
               component={item.component} />
      )
      return allRoutes
    })
    return (
      <Switch>
        {allRoutes}
        <Redirect from="*" to="/" key="default-route" />
      </Switch>
    )
  }

  render () {
    return (
      <React.Fragment>
        <Masthead
          iconImg={logo}
          navToggle
          thin
          >
          <div className="collapse navbar-collapse">
            {this.renderMenu()}
            <ul className="nav navbar-nav navbar-utility">
              <li><a href="https://zuul-ci.org/docs"
                     rel="noopener noreferrer" target="_blank">
                  Documentation
              </a></li>
            </ul>
          </div>
        </Masthead>
        <div className="container-fluid container-cards-pf">
          {this.renderContent()}
        </div>
      </React.Fragment>
    )
  }
}

export default withRouter(App)
