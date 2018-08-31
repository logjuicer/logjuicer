/* global setTimeout, clearTimeout */
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
import { connect } from 'react-redux'
import { Icon } from 'patternfly-react'

import { fetchStatusAction } from '../reducers'


class StatusPage extends React.Component {
  static propTypes = {
    status: PropTypes.object,
    dispatch: PropTypes.func
  }

  componentDidMount () {
    this.refresh()
  }

  refresh = () => {
    this.props.dispatch(fetchStatusAction())
    if (this.timer) {
      clearTimeout(this.timer)
    }
    this.timer = setTimeout(this.refresh, 5000)
  }

  componentWillUnmount () {
    if (this.timer) {
      clearTimeout(this.timer)
      this.timer = null
    }
  }

  renderJobs (jobs) {
    return (
      <React.Fragment>
        <h2>Running jobs</h2>
        <ul className="list-group">
          {Object.values(jobs).map((item, idx) => (
            <li
              className="list-group-item"
              key={idx}>{JSON.stringify(item)}</li>
          ))}
        </ul>
      </React.Fragment>
    )
  }

  renderHistory (history) {
    return (
      <React.Fragment>
        <h2>History</h2>
         <ul className="list-group">
          {history.map((item, idx) => (
            <li
              className="list-group-item"
              key={idx}>{item}</li>
          ))}
        </ul>
      </React.Fragment>
    )
  }

  renderStatus (status) {
    return (
      <React.Fragment>
        <h2>Functions</h2>
        <table
          className="table table-condensed table-responsive table-bordered">
          <thead>
            <tr>
              <th>Function</th><th>Queue</th><th>Running</th><th>Workers</th>
            </tr>
          </thead>
          <tbody>
            {Object.keys(status.functions).map(item => (
              <tr key={item}>
                <td>{item}</td>
                <td>{status.functions[item][0]}</td>
                <td>{status.functions[item][1]}</td>
                <td>{status.functions[item][2]}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {Object.values(status.jobs).length > 0 &&
         this.renderJobs(status.jobs)}
        {status.history.length > 0 &&
         this.renderHistory(status.history)}
      </React.Fragment>
    )
  }

  render () {
    const { status } = this.props
    return (
      <React.Fragment>
        <a className="refresh pull-right" onClick={this.refresh}>
          <Icon type="fa" name="refresh" /> refresh
        </a>
        {status ? this.renderStatus(status) : <p>Loading</p>}
      </React.Fragment>
    )
  }
}

export default connect(
  state => ({
    status: state.status
  })
)(StatusPage)
